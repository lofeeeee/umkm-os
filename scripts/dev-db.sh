#!/usr/bin/env bash
# Postgres lokal untuk development/tes (pgvector/pgvector:pg16, PRD.md §19 #11).
# Container terpisah dari service lain di mesin; port host 55432; data ephemeral.
#   scripts/dev-db.sh up     -> jalankan + tunggu siap, cetak export DATABASE_URL
#   scripts/dev-db.sh down   -> hapus container
set -euo pipefail
NAME=umkm-os-pg-test
PORT=${UMKM_DEV_DB_PORT:-55432}
# Kredensial ini HANYA untuk container lokal sekali pakai, bukan environment nyata.
USER=umkm_owner PASS=umkm_dev_only DB=umkm_dev
case "${1:-up}" in
  up)
    if ! docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
      docker rm -f "$NAME" >/dev/null 2>&1 || true
      docker run -d --name "$NAME" -p "127.0.0.1:$PORT:5432" \
        -e POSTGRES_USER=$USER -e POSTGRES_PASSWORD=$PASS -e POSTGRES_DB=$DB \
        --tmpfs /var/lib/postgresql/data pgvector/pgvector:pg16 >/dev/null
    fi
    for _ in $(seq 1 60); do
      docker exec "$NAME" pg_isready -U $USER -d $DB -q && break; sleep 0.5
    done
    docker exec "$NAME" pg_isready -U $USER -d $DB -q
    echo "export DATABASE_URL=postgres://$USER:$PASS@127.0.0.1:$PORT/$DB"
    ;;
  down) docker rm -f "$NAME" >/dev/null && echo "removed $NAME" ;;
  *) echo "usage: $0 up|down" >&2; exit 2 ;;
esac
