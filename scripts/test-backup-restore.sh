#!/usr/bin/env bash
# Uji backup -> restore ke DB kosong + negative control (P0-10).
# Memakai container scripts/dev-db.sh dan image ops/backup. Tidak menyentuh DB lain.
set -euo pipefail
cd "$(dirname "$0")/.."
eval "$(scripts/dev-db.sh up)"
PG=umkm-os-pg-test
IMG=umkm-os/backup:local
docker build -q -f ops/backup/Dockerfile -t "$IMG" . >/dev/null

work=$(mktemp -d); chmod 777 "$work"; mkdir -m 777 "$work/backups" "$work/keys"
trap 'docker run --rm -v "$work:/w" alpine:3 rm -rf /w/backups /w/keys >/dev/null 2>&1; rm -rf "$work"' EXIT
psql_c() { docker exec -i "$PG" psql -XAtq -U umkm_owner -d "$1" -v ON_ERROR_STOP=1 -c "$2"; }
run_bk() { # db, extra docker args..., cmd...
  local db=$1; shift
  docker run --rm --network "container:$PG" -v "$work:/w" \
    -e PGHOST=127.0.0.1 -e PGUSER=umkm_owner -e PGPASSWORD=umkm_dev_only -e PGDATABASE="$db" \
    -e BACKUP_DIR=/w/backups "$@"
}
pass=0; fail=0
# expect <desc> <expected_exit> <command...>: jalankan command, bandingkan exit code.
expect() {
  local desc=$1 want=$2 got; shift 2
  "$@"; got=$?
  if [[ "$want" == "$got" ]]; then echo "PASS  $desc (exit $got)"; pass=$((pass+1)); else echo "FAIL  $desc (expected $want, got $got)"; fail=$((fail+1)); fi
}
count() { local n=0 f; for f in "$@"; do [[ -e "$f" ]] && n=$((n+1)); done; echo "$n"; }
newest() { local f last=""; for f in "$1"/*.dump.age; do [[ -e "$f" ]] && last=$(basename "$f"); done; echo "$last"; }

for db in bk_src bk_dst bk_dst2; do psql_c umkm_dev "DROP DATABASE IF EXISTS $db" ; psql_c umkm_dev "CREATE DATABASE $db"; done
DATABASE_URL="${DATABASE_URL%/*}/bk_src" sqlx migrate run --source migrations >/dev/null
psql_c bk_src "CREATE TABLE demo_orders (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), tenant_id uuid NOT NULL,
  total NUMERIC(14,2) NOT NULL, note text NOT NULL, created_at timestamptz NOT NULL DEFAULT now());
  INSERT INTO demo_orders (tenant_id, total, note)
  SELECT gen_random_uuid(), (random()*1000000)::numeric(14,2), 'SENTINEL-PLAINTEXT-' || g FROM generate_series(1,5000) g;"
sum_src=$(psql_c bk_src "SELECT count(*) || ':' || md5(string_agg(id::text||tenant_id||total||note, ',' ORDER BY id)) FROM demo_orders")
echo "source: $sum_src"

run_bk bk_src -v "$work/keys:/k" "$IMG" age-keygen -o /k/id.txt 2>/dev/null
run_bk bk_src -v "$work/keys:/k" "$IMG" age-keygen -o /k/wrong.txt 2>/dev/null
recipient=$(run_bk bk_src -v "$work/keys:/k" "$IMG" age-keygen -y /k/id.txt)

set +e
expect "backup harian" 0 run_bk bk_src -e BACKUP_AGE_RECIPIENT="$recipient" -e BACKUP_ALLOW_LOCAL_ONLY=1 "$IMG" backup.sh
file=$(newest "$work/backups/daily")
expect "arsip tidak memuat plaintext" 1 grep -q "SENTINEL-PLAINTEXT" "$work/backups/daily/$file"

expect "restore ke DB kosong" 0 run_bk bk_dst -v "$work/keys:/k:ro" "$IMG" restore.sh "/w/backups/daily/$file" /k/id.txt
sum_dst=$(psql_c bk_dst "SELECT count(*) || ':' || md5(string_agg(id::text||tenant_id||total||note, ',' ORDER BY id)) FROM demo_orders")
echo "restored: $sum_dst"
expect "row count + checksum data identik" 0 test "$sum_src" = "$sum_dst"
expect "riwayat migration ikut ter-restore" 0 test "$(psql_c bk_dst 'SELECT count(*) FROM _sqlx_migrations WHERE success')" = "$(psql_c bk_src 'SELECT count(*) FROM _sqlx_migrations WHERE success')"
expect "fungsi app.current_tenant_id ter-restore" 0 test "$(psql_c bk_dst 'SELECT app.current_tenant_id() IS NULL')" = "t"

echo "--- negative controls ---"
expect "target tidak kosong ditolak" 4 run_bk bk_dst -v "$work/keys:/k:ro" "$IMG" restore.sh "/w/backups/daily/$file" /k/id.txt
expect "identity salah ditolak" 1 run_bk bk_dst2 -v "$work/keys:/k:ro" "$IMG" restore.sh "/w/backups/daily/$file" /k/wrong.txt
cp "$work/backups/daily/$file" "$work/tampered.dump.age"; cp "$work/backups/daily/$file.sha256" "$work/tampered.dump.age.sha256"
sed -i 's/'"$file"'/tampered.dump.age/' "$work/tampered.dump.age.sha256"
printf '\xff' | dd of="$work/tampered.dump.age" bs=1 seek=4000 conv=notrunc status=none
expect "arsip diubah: checksum gagal" 1 run_bk bk_dst2 -v "$work/keys:/k:ro" "$IMG" restore.sh /w/tampered.dump.age /k/id.txt
rm "$work/tampered.dump.age.sha256"
expect "arsip diubah tanpa .sha256: autentikasi age gagal" 1 run_bk bk_dst2 -v "$work/keys:/k:ro" "$IMG" restore.sh /w/tampered.dump.age /k/id.txt
expect "target tetap kosong setelah restore gagal" 0 test "$(psql_c bk_dst2 "SELECT count(*) FROM pg_tables WHERE schemaname='public'")" = "0"
expect "tanpa off-site & tanpa ALLOW_LOCAL_ONLY -> exit 3" 3 run_bk bk_src -e BACKUP_AGE_RECIPIENT="$recipient" "$IMG" backup.sh
expect "recipient bukan public key age ditolak" 2 run_bk bk_src -e BACKUP_AGE_RECIPIENT="not-a-key" -e BACKUP_ALLOW_LOCAL_ONLY=1 "$IMG" backup.sh

echo "--- retensi (KEEP_DAILY=7, KEEP_WEEKLY=4, weekly = hari ini) ---"
dow=$(date -u +%u)
for _ in $(seq 1 9); do
  sleep 1
  run_bk bk_src -e BACKUP_AGE_RECIPIENT="$recipient" -e BACKUP_ALLOW_LOCAL_ONLY=1 -e BACKUP_WEEKLY_DOW="$dow" "$IMG" backup.sh >/dev/null
done
d=$(count "$work"/backups/daily/*.dump.age); w=$(count "$work"/backups/weekly/*.dump.age)
s=$(count "$work"/backups/daily/*.sha256)
echo "daily=$d weekly=$w daily_sha=$s"
expect "rotasi 7 daily + 4 weekly" 0 test "$d-$w-$s" = "7-4-7"
expect "rotasi menghapus arsip TERLAMA (backup pertama hilang)" 1 test -e "$work/backups/daily/$file"
last_created=$(newest "$work/backups/daily")
expect "arsip terbaru tetap ada" 0 test -e "$work/backups/daily/$last_created"
oldest_kept=""; for f in "$work"/backups/daily/*.dump.age; do oldest_kept=$(basename "$f"); break; done
echo "first=$file oldest_kept=$oldest_kept newest=$last_created"
expect "arsip yang tersisa semuanya lebih baru dari yang dihapus" 0 test "$oldest_kept" \> "$file"
set -e

for db in bk_src bk_dst bk_dst2; do psql_c umkm_dev "DROP DATABASE IF EXISTS $db"; done
echo "RESULT: $pass pass, $fail fail"
[[ $fail == 0 ]]
