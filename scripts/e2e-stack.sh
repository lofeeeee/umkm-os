#!/usr/bin/env bash
# E2E Playwright terhadap stack NYATA, terisolasi dari stack dev (port/DB/role/Redis sendiri):
#   PostgreSQL + Redis dari scripts/dev-db.sh, DB `umkm_e2e` + login `umkm_e2e_app` (NOBYPASSRLS),
#   API :18080 + worker (metrics 19090/19091), provider AI tiruan :18181 (frontend/e2e/mock-llm.py),
#   Caddy (network host, :80) dengan ops/caddy/Caddyfile (api:8080 → 127.0.0.1:18080), domain umkm.test.
#
# Pemakaian:
#   scripts/e2e-stack.sh up        # build (cargo + frontend), migrasi, jalankan semua proses
#   scripts/e2e-stack.sh test [spec…]   # jalankan spec (default: semua) satu per satu
#   scripts/e2e-stack.sh down      # hentikan proses & Caddy (DB e2e dibiarkan)
#
# Catatan runner (bukan perilaku produk):
# - Paket Starter di DB e2e diberi 200.000 token AI platform agar ai/chat.spec bisa menguji percakapan
#   (produk: Starter = 0, dikunci apps/api/tests/plan_limits_http.rs).
# - Limiter registrasi/login per IP dikosongkan sebelum tiap spec (semua spec mendaftar dari 127.0.0.1).
# - Super admin uji (admin-e2e@x.id) dikembalikan ke "belum MFA" sebelum billing.spec.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT_DIR=$PWD
W=${E2E_WORKDIR:-$ROOT_DIR/target/e2e}
mkdir -p "$W"
PG=umkm-os-pg-test
RD=umkm-os-redis-test
PW=umkm_dev_only

pgq() { docker exec -i "$PG" psql -q -v ON_ERROR_STOP=1 -U umkm_owner -d "$1"; }
ready() { for _ in $(seq 1 40); do curl -sf -H "Host: api.umkm.test" http://127.0.0.1/readyz >/dev/null && return 0; sleep 1; done; return 1; }

write_config() {
  for c in config config-worker; do
    rm -rf "${W:?}/$c"; cp -r config "$W/$c"
    python3 -I - "$W/$c/development.toml" "$ROOT_DIR" "$c" <<'PY'
import re, sys
p, root, kind = sys.argv[1:4]
s = open(p).read()
s = s.replace("port = 8080", "port = 18080", 1)
s = re.sub(r'cors_allowed_origins = \[[^\]]*\]', 'cors_allowed_origins = ["http://app.umkm.test"]', s)
s = re.sub(r'verify_url_base = "[^"]*"', 'verify_url_base = "http://app.umkm.test/verify-email"', s)
s = re.sub(r'(\[metrics\][^\[]*?port = )\d+', r'\g<1>' + ("19091" if kind == "config-worker" else "19090"), s, flags=re.S)
s = re.sub(r'key_prefix = "[^"]*"', 'key_prefix = "umkme2e"', s)
s = re.sub(r'root_domain = "[^"]*"', 'root_domain = "umkm.test"', s)
s = re.sub(r'(\[ai\.platform\][^\[]*?)enabled = false\nbase_url = ""\nmodels = \[\]\nembedding_model = ""',
           r'\1enabled = true\nbase_url = "http://127.0.0.1:18181/v1"\nmodels = ["mock-1"]\nembedding_model = "mock-embed"\ntool_calling = true\nembeddings = true\ntrusted_internal = true', s, flags=re.S)
s = re.sub(r'shell_path = "[^"]*"', f'shell_path = "{root}/frontend/out/site.html"', s)
open(p, "w").write(s)
PY
  done
}

env_file() {
  [ -f "$W/env.sh" ] || cat > "$W/env.sh" <<EOF
export UMKM_ENV=development
export UMKM_REDIS_URL='redis://:$PW@127.0.0.1:56379/5'
export UMKM_FILE_SIGNING_KEY='e2e-only-file-signing-key-$(head -c16 /dev/urandom | od -An -tx1 | tr -d ' \n')'
export UMKM_DATA_KEK='v1:$(head -c32 /dev/urandom | base64)'
export UMKM_PLATFORM_AI_API_KEY='e2e-mock'
export OWNER_DB='postgres://umkm_owner:$PW@127.0.0.1:55432/umkm_e2e'
export APP_DB='postgres://umkm_e2e_app:e2e_only_pw@127.0.0.1:55432/umkm_e2e'
EOF
  # shellcheck source=/dev/null
  . "$W/env.sh"
}

start_api() {
  if [ -f "$W/api.pid" ]; then kill "$(cat "$W/api.pid")" 2>/dev/null || true; sleep 1; fi
  UMKM_CONFIG_DIR=$W/config UMKM_DATABASE_URL=$APP_DB nohup target/debug/umkm-api serve > "$W/api.log" 2>&1 &
  echo $! > "$W/api.pid"
}

start_caddy() {
  sed 's/api:8080/127.0.0.1:18080/g' ops/caddy/Caddyfile > "$W/Caddyfile"
  docker rm -f umkm-e2e-caddy >/dev/null 2>&1 || true
  docker run -d --rm --name umkm-e2e-caddy --network host -e ROOT_DOMAIN=umkm.test -e PUBLIC_API_ORIGIN=http://api.umkm.test \
    -v "$ROOT_DIR/frontend/out:/srv/www:ro" -v "$W/Caddyfile:/etc/caddy/Caddyfile:ro" caddy:2.10-alpine >/dev/null
}

up() {
  eval "$(scripts/dev-db.sh up)"
  pgq umkm_dev <<'SQL'
DROP DATABASE IF EXISTS umkm_e2e WITH (FORCE);
CREATE DATABASE umkm_e2e;
DO $$ BEGIN CREATE ROLE umkm_e2e_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD 'e2e_only_pw';
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
GRANT umkm_app_role TO umkm_e2e_app;
GRANT CONNECT ON DATABASE umkm_e2e TO umkm_e2e_app;
SQL
  write_config; env_file
  cargo build -q -p umkm-api -p umkm-worker
  (cd frontend && NEXT_PUBLIC_API_ORIGIN=http://api.umkm.test NEXT_TELEMETRY_DISABLED=1 npm run build >/dev/null)
  UMKM_CONFIG_DIR=$W/config UMKM_DATABASE_URL=$OWNER_DB target/debug/umkm-api migrate
  pgq umkm_e2e <<'SQL'
UPDATE plans SET limits_json = jsonb_set(limits_json, '{platform_ai_tokens_monthly}', '200000') WHERE code = 'starter';
SQL
  nohup python3 -I frontend/e2e/mock-llm.py 18181 > "$W/llm.log" 2>&1 & echo $! > "$W/llm.pid"
  UMKM_CONFIG_DIR=$W/config-worker UMKM_DATABASE_URL=$APP_DB nohup target/debug/umkm-worker > "$W/worker.log" 2>&1 &
  echo $! > "$W/worker.pid"
  # API membaca site.html saat start → selalu SETELAH build frontend (ADR-0018).
  start_api; start_caddy; ready
  curl -s -H "Host: api.umkm.test" -H "Origin: http://app.umkm.test" -H 'content-type: application/json' \
    http://127.0.0.1/api/v1/auth/register -o /dev/null \
    -d '{"email":"admin-e2e@x.id","password":"rahasia-admin-123","business_name":"Platform","slug":"platformadm","accept_terms":true,"accept_privacy":true}'
  pgq umkm_e2e <<<"UPDATE users SET platform_role = 'super_admin' WHERE email = 'admin-e2e@x.id';"
  echo "stack e2e siap (log: $W)"
}

reset_limits() {
  docker exec "$RD" sh -c "redis-cli -a $PW -n 5 --no-auth-warning --scan --pattern 'umkme2e:platform:rl_*' \
    | xargs -r redis-cli -a $PW -n 5 --no-auth-warning del" >/dev/null
}

# Toko v1 terbit untuk dashboard/responsive.spec (butuh SITE_URL + akun yang sudah ada).
seed_v1_shop() {
  local slug j r csrf t rev
  slug=dash$(date +%s | tail -c 7); j=$W/cookies.txt; rm -f "$j"
  local a=(-s -b "$j" -c "$j" -H "Host: api.umkm.test" -H "Origin: http://app.umkm.test" -H 'content-type: application/json')
  r=$(curl "${a[@]}" http://127.0.0.1/api/v1/auth/register -d "{\"email\":\"$slug@x.id\",\"password\":\"rahasia-dash-123\",\"business_name\":\"Toko Dash\",\"slug\":\"$slug\",\"accept_terms\":true,\"accept_privacy\":true}")
  csrf=$(python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["csrf_token"])' <<<"$r")
  t=$(python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["tenant"]["id"])' <<<"$r")
  curl "${a[@]}" -H "x-csrf-token: $csrf" -X PUT "http://127.0.0.1/api/v1/tenants/$t/website" -d '{"template_key":"modern_store"}' -o /dev/null
  rev=$(curl "${a[@]}" "http://127.0.0.1/api/v1/tenants/$t/website/pages/home/draft" | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["revision"])')
  curl "${a[@]}" -H "x-csrf-token: $csrf" -X POST "http://127.0.0.1/api/v1/tenants/$t/website/pages/home/publish" -d "{\"revision\":$rev}" -o /dev/null
  echo "$slug"
}

run_tests() {
  env_file
  local specs=("$@") fail=0 slug
  [ ${#specs[@]} -gt 0 ] || mapfile -t specs < <(cd frontend && ls e2e/*.spec.ts)
  reset_limits; slug=$(seed_v1_shop)
  for s in "${specs[@]}"; do
    reset_limits
    [[ "$s" == *billing* ]] && pgq umkm_e2e <<'SQL'
DELETE FROM mfa_recovery_codes WHERE user_id = (SELECT id FROM users WHERE email = 'admin-e2e@x.id');
DELETE FROM mfa_challenges WHERE user_id = (SELECT id FROM users WHERE email = 'admin-e2e@x.id');
DELETE FROM mfa_credentials WHERE user_id = (SELECT id FROM users WHERE email = 'admin-e2e@x.id');
SQL
    if ! (cd frontend && E2E_HOST_MAP=umkm.test APP_URL=http://app.umkm.test API_URL=http://api.umkm.test ROOT=umkm.test \
      E2E_AI=1 E2E_ADMIN_EMAIL=admin-e2e@x.id E2E_ADMIN_PASSWORD=rahasia-admin-123 \
      SITE_URL="http://$slug.umkm.test" E2E_EMAIL="$slug@x.id" E2E_PASSWORD=rahasia-dash-123 \
      npx playwright test "$s" --reporter=line --workers=1); then fail=1; fi
  done
  return $fail
}

down() {
  for p in api worker llm; do [ -f "$W/$p.pid" ] && kill "$(cat "$W/$p.pid")" 2>/dev/null || true; rm -f "$W/$p.pid"; done
  docker rm -f umkm-e2e-caddy >/dev/null 2>&1 || true
  echo "stack e2e dihentikan"
}

case "${1:-}" in
  up) up ;;
  test) shift; run_tests "$@" ;;
  restart-api) env_file; start_api; start_caddy; ready && echo "api & caddy e2e dimulai ulang" ;;
  down) down ;;
  *) echo "usage: $0 up|test [spec…]|restart-api|down" >&2; exit 2 ;;
esac
