#!/bin/sh
# Restore backup terenkripsi ke database KOSONG (P0-10, runbook: docs/governance/runbook-restore.md).
#
#   restore.sh <file.dump.age> <age-identity-file>
#
# Env: PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE (database TARGET; harus sudah ada dan kosong).
# Menolak target yang sudah berisi tabel (fail-closed; tidak ada mode --force).
set -eu

log() { printf '{"ts":"%s","component":"restore","level":"%s","msg":"%s"}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2"; }
die() { log error "$2"; exit "$1"; }

[ $# -eq 2 ] || die 2 "usage: restore.sh <file.dump.age> <age-identity-file>"
file=$1 identity=$2
[ -f "$file" ] || die 2 "file tidak ada: $file"
[ -f "$identity" ] || die 2 "identity age tidak ada"
for v in PGHOST PGUSER PGPASSWORD PGDATABASE; do
  eval "val=\${$v:-}"; [ -n "$val" ] || die 2 "env $v wajib"
done

if [ -f "$file.sha256" ]; then
  (cd "$(dirname "$file")" && sha256sum -c "$(basename "$file").sha256" >/dev/null) \
    || die 1 "checksum tidak cocok: arsip rusak/diubah"
  log info "checksum ok"
else
  log warn "tanpa file .sha256; integritas hanya dijamin autentikasi age"
fi

existing=$(psql -XAtq -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
   WHERE c.relkind IN ('r','p','v','m','S','f') AND n.nspname NOT IN ('pg_catalog','information_schema')
     AND n.nspname NOT LIKE 'pg_toast%' AND n.nspname NOT LIKE 'pg_temp%'") \
  || die 1 "tidak dapat terhubung ke target"
[ "$existing" = "0" ] || die 4 "target $PGDATABASE tidak kosong ($existing relasi); restore ditolak"

mkdir -p "${TMPDIR:-/tmp}"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/restore.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM
age --decrypt --identity "$identity" --output "$tmp/plain.dump" "$file" \
  || die 1 "dekripsi gagal (key salah atau arsip rusak)"

start=$(date -u +%s)
pg_restore --exit-on-error --no-owner --no-privileges --single-transaction --dbname="$PGDATABASE" "$tmp/plain.dump" \
  || die 1 "pg_restore gagal"
log info "restore ok db=$PGDATABASE seconds=$(( $(date -u +%s) - start ))"

psql -XAtq -F ' ' -c "SELECT version, success FROM _sqlx_migrations ORDER BY version" 2>/dev/null \
  | while read -r v ok; do log info "migration version=$v success=$ok"; done || true
