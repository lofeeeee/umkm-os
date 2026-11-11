#!/bin/sh
# Jalankan backup.sh sekali per hari pada BACKUP_AT_UTC (default 02:00). Non-root, tanpa crond.
# `last` mencegah run ganda di hari yang sama bila sleep/backup selesai sebelum menit target lewat.
set -u
at=${BACKUP_AT_UTC:-02:00}
last=""
log() { printf '{"ts":"%s","component":"backup","level":"%s","msg":"%s"}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2"; }
while :; do
  now=$(date -u +%s)
  today=$(date -u +%Y-%m-%d)
  next=$(date -u -d "$today $at" +%s 2>/dev/null) || { log error "BACKUP_AT_UTC tidak valid: $at"; exit 2; }
  if [ "$next" -le "$now" ] || [ "$last" = "$today" ]; then
    next=$((next + 86400))
  fi
  log info "next run in $((next - now))s"
  sleep $((next - now))
  last=$(date -u -d "@$next" +%Y-%m-%d)
  backup.sh || log error "backup run failed exit=$?"
done
