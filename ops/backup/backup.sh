#!/bin/sh
# Backup logis harian PostgreSQL (P0-10, PRD.md §6.10).
#
#   pg_dump -Fc  ->  pg_restore --list (validasi)  ->  age (enkripsi public key)  ->  rotasi
#
# Env wajib:
#   PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE   koneksi sumber
#   BACKUP_DIR                                   direktori staging lokal (volume)
#   BACKUP_AGE_RECIPIENT                         public key age (age1...). Private key TIDAK ada di server.
# Opsional:
#   BACKUP_KEEP_DAILY=7 BACKUP_KEEP_WEEKLY=4     retensi PRD.md §6.10
#   BACKUP_WEEKLY_DOW=7                          hari (1=Senin..7=Minggu, UTC) salinan mingguan
#   BACKUP_OFFSITE_CMD                           perintah salin ke luar VPS; dipanggil dengan path file.
#                                                Menunggu D-07. Tanpa ini script EXIT 3 (backup belum aman),
#   BACKUP_ALLOW_LOCAL_ONLY=1                    ...kecuali untuk dev/test.
#
# Keterbatasan (jujur): dump harian => RPO ~24 jam. Target RPO 15 menit butuh WAL archiving
# ke storage luar VPS (menunggu D-07). RPO/RTO TIDAK diklaim sampai drill P8-4.
set -eu

log() { printf '{"ts":"%s","component":"backup","level":"%s","msg":"%s"}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2"; }
die() { log error "$2"; exit "$1"; }

for v in PGHOST PGUSER PGPASSWORD PGDATABASE BACKUP_DIR BACKUP_AGE_RECIPIENT; do
  eval "val=\${$v:-}"; [ -n "$val" ] || die 2 "env $v wajib"
done
case "$BACKUP_AGE_RECIPIENT" in age1*) ;; *) die 2 "BACKUP_AGE_RECIPIENT harus public key age (age1...)";; esac

KEEP_DAILY=${BACKUP_KEEP_DAILY:-7}
KEEP_WEEKLY=${BACKUP_KEEP_WEEKLY:-4}
WEEKLY_DOW=${BACKUP_WEEKLY_DOW:-7}
mkdir -p "$BACKUP_DIR/daily" "$BACKUP_DIR/weekly"

stamp=$(date -u +%Y%m%dT%H%M%SZ)
name="${PGDATABASE}-${stamp}.dump.age"
mkdir -p "${TMPDIR:-/tmp}"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/backup.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM

log info "dump start db=$PGDATABASE"
pg_dump --format=custom --compress=6 --no-owner --no-privileges --file="$tmp/plain.dump" \
  || die 1 "pg_dump gagal"
# Validasi arsip dapat dibaca sebelum dienkripsi.
pg_restore --list "$tmp/plain.dump" > "$tmp/toc.txt" || die 1 "arsip dump tidak valid"
entries=$(grep -vc '^;' "$tmp/toc.txt" || true)

age --encrypt --recipient "$BACKUP_AGE_RECIPIENT" --output "$tmp/$name" "$tmp/plain.dump" \
  || die 1 "enkripsi age gagal"
rm -f "$tmp/plain.dump"
(cd "$tmp" && sha256sum "$name" > "$name.sha256")
mv "$tmp/$name" "$tmp/$name.sha256" "$BACKUP_DIR/daily/"
size=$(wc -c < "$BACKUP_DIR/daily/$name")
log info "dump ok file=daily/$name bytes=$size toc_entries=$entries"

if [ "$(date -u +%u)" = "$WEEKLY_DOW" ]; then
  cp "$BACKUP_DIR/daily/$name" "$BACKUP_DIR/daily/$name.sha256" "$BACKUP_DIR/weekly/"
  log info "weekly copy file=weekly/$name"
fi

# Rotasi: simpan N terbaru (nama ber-timestamp UTC => urut leksikografis = urut waktu).
prune() {
  dir=$1 keep=$2
  # Glob terurut naik = terlama dulu; hapus kelebihan dari depan.
  set -- "$dir"/*.dump.age
  [ -e "$1" ] || return 0
  excess=$(($# - keep))
  for f in "$@"; do
    [ "$excess" -gt 0 ] || break
    rm -f "$f" "$f.sha256"; log info "pruned $(basename "$dir")/$(basename "$f")"
    excess=$((excess - 1))
  done
}
prune "$BACKUP_DIR/daily" "$KEEP_DAILY"
prune "$BACKUP_DIR/weekly" "$KEEP_WEEKLY"

if [ -n "${BACKUP_OFFSITE_CMD:-}" ]; then
  sh -c "$BACKUP_OFFSITE_CMD \"\$1\" \"\$1.sha256\"" offsite "$BACKUP_DIR/daily/$name" \
    || die 1 "salin off-site gagal"
  log info "offsite ok file=daily/$name"
elif [ "${BACKUP_ALLOW_LOCAL_ONLY:-0}" = "1" ]; then
  log warn "OFFSITE NOT CONFIGURED (BACKUP_ALLOW_LOCAL_ONLY=1, hanya dev/test)"
else
  die 3 "OFFSITE NOT CONFIGURED: backup hanya ada di VPS (PRD.md §6.10 mewajibkan di luar VPS; menunggu D-07)"
fi
