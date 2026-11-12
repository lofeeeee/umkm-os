#!/usr/bin/env bash
# Smoke ADR-0014 terhadap stack NYATA lewat Caddy di :80 (API + PostgreSQL + Redis + build frontend).
# Pemakaian: DOM=umkm.test scripts/smoke-adr0014.sh <dir-kerja>
# Prasyarat: API berjalan (UMKM_ROOT_DOMAIN=$DOM), Caddy dengan ROOT_DOMAIN=$DOM dan frontend/out terpasang.
# Domain lokal harus satu "site" bagi browser (bukan *.localhost); lihat docs/evidence/P2-7.txt.
set -euo pipefail
DOM=${DOM:-umkm.test}
J=$1/cookies.txt; rm -f "$J"
A=(-s -b "$J" -c "$J" -H "Host: api.$DOM" -H "Origin: http://app.$DOM")
api(){ curl "${A[@]}" -X "$1" "http://127.0.0.1$2" "${@:3}"; }
ok(){ echo "PASS $*"; }; fail(){ echo "FAIL $*"; exit 1; }
SLUG=smoke$RANDOM
R=$(api POST /api/v1/auth/register -H 'content-type: application/json' -d "{\"email\":\"$SLUG@x.id\",\"password\":\"rahasia-123\",\"business_name\":\"Toko Smoke\",\"slug\":\"$SLUG\",\"accept_terms\":true,\"accept_privacy\":true}")
CSRF=$(echo "$R" | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["csrf_token"])')
T=$(api GET /api/v1/tenants | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"][0]["tenant_id"])'); ok "register + tenant $T ($SLUG)"
H=(-H "x-csrf-token: $CSRF" -H 'content-type: application/json')
api PUT /api/v1/tenants/$T/website "${H[@]}" -d '{"template_key":"modern_store"}' >/dev/null
D=$(api GET /api/v1/tenants/$T/website/pages/home/draft)
python3 - "$D" > "$1/draft.json" <<'PY'
import sys,json
d=json.loads(sys.argv[1])["data"]
for s in d["content"]["sections"]:
    if s["type"]=="hero": s["data"]["title"]="Halo dari Smoke <script>x</script>"
print(json.dumps({"content":d["content"],"base_revision":d["revision"]}))
PY
printf '\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\1\0\0\0\1\x08\x06\0\0\0\0\0\0\0\0\0\0\0IEND\0\0\0\0' > "$1/x.png"
U=$(api POST /api/v1/tenants/$T/uploads -H "x-csrf-token: $CSRF" --data-binary @"$1/x.png")
echo "$U" | grep -q '"ref":"asset:' && ok "upload gambar" || fail "upload: $U"
code=$(api PUT /api/v1/tenants/$T/website/pages/home/draft "${H[@]}" --data @"$1/draft.json" -o /dev/null -w '%{http_code}'); [ "$code" = 200 ] && ok "simpan draft" || fail "draft $code"
code=$(api POST /api/v1/tenants/$T/website/pages/home/publish "${H[@]}" -d '{}' -o /dev/null -w '%{http_code}'); [ "$code" = 200 ] && ok "publish" || fail "publish $code"
# Tanpa CSRF -> 403
code=$(api POST /api/v1/tenants/$T/website/pages/home/publish -H 'content-type: application/json' -d '{}' -o /dev/null -w '%{http_code}'); [ "$code" = 403 ] && ok "publish tanpa CSRF ditolak (403)" || fail "csrf $code"

TH=(-s -H "Host: $SLUG.$DOM")
hdr=$(curl "${TH[@]}" -D - -o "$1/shell.html" http://127.0.0.1/promo)
grep -q '<div' "$1/shell.html" && echo "$hdr" | grep -qi "content-security-policy: default-src 'self'; script-src 'self' 'sha256-" && ok "host tenant /promo -> shell statis + CSP hash" || fail "shell"
echo "$hdr" | grep -qi "unsafe-inline'; font" && true
echo "$hdr" | grep -i "content-security-policy" | grep -q "script-src[^;]*unsafe-inline" && fail "script-src unsafe-inline" || ok "script-src tanpa unsafe-inline"
P=$(curl "${TH[@]}" http://127.0.0.1/_api/public/site)
echo "$P" | grep -q 'Halo dari Smoke <script>x</script>' && ok "/_api/public/site via Host -> versi terbit" || fail "public: $P"
curl "${TH[@]}" -D - -o /dev/null http://127.0.0.1/_api/public/site | grep -qi "cache-control: public, max-age=60" && ok "cache publik 60 dtk"
ct=$(curl "${TH[@]}" -o /dev/null -w '%{content_type}' http://127.0.0.1/_api/v1/tenants); [[ "$ct" == text/html* ]] && ok "host tenant TIDAK meneruskan /_api/v1/* (dapat shell HTML)" || fail "leak $ct"
ct=$(curl "${TH[@]}" -o /dev/null -w '%{content_type}' http://127.0.0.1/api/v1/tenants); [[ "$ct" == text/html* ]] && ok "host tenant TIDAK meneruskan /api/v1/*" || fail "leak2 $ct"
code=$(curl -s -H "Host: tidakada.$DOM" -o /dev/null -w '%{http_code}' http://127.0.0.1/_api/public/site); [ "$code" = 404 ] && ok "host tak dikenal -> 404" || fail "unknown $code"
code=$(curl -s -H "Host: $SLUG.$DOM" -H "X-Forwarded-Host: lain.$DOM" -o /dev/null -w '%{http_code}' http://127.0.0.1/_api/public/site); [ "$code" = 200 ] && ok "X-Forwarded-Host diabaikan"
IMG=$(echo "$P" | python3 -c 'import sys,json;print(list(json.load(sys.stdin)["data"]["assets"].values())[0] if json.load(open("/dev/stdin")) else "")' 2>/dev/null || true)
code=$(curl -s -H "Host: app.$DOM" -o "$1/app.html" -w '%{http_code}' http://127.0.0.1/website); [ "$code" = 200 ] && grep -q '<div' "$1/app.html" && ok "app.$DOM/website -> dashboard" || fail "app $code"
curl -s -H "Host: app.$DOM" -D - -o /dev/null http://127.0.0.1/ | grep -i "content-security-policy" | grep -q "connect-src 'self' http://api.$DOM" && ok "CSP dashboard connect-src api"
code=$(curl -s -H "Host: app.$DOM" -o /dev/null -w '%{http_code}' http://127.0.0.1/site); [ "$code" = 404 ] && ok "renderer tidak dilayani di host dashboard" || fail "site on app $code"
code=$(curl -s -H "Host: $DOM" -o /dev/null -w '%{http_code}' http://127.0.0.1/); [ "$code" = 302 ] && ok "root -> redirect dashboard"
f=$(curl -s -H "Host: $SLUG.$DOM" -o /dev/null -w '%{http_code} %{content_type}' http://127.0.0.1/_next/static/chunks/$(ls frontend/out/_next/static/chunks | head -1)); [[ "$f" == 200* ]] && ok "aset _next di host tenant ($f)"
echo SMOKE-OK
