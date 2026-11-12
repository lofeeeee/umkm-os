#!/usr/bin/env bash
# Postgres + Redis lokal untuk development/tes (pgvector:pg16, redis:7.4).
# Container terpisah dari service lain di mesin; port host 55432 / 56379; data ephemeral.
#   scripts/dev-db.sh up     -> jalankan + tunggu siap, cetak export DATABASE_URL & REDIS_URL
#   scripts/dev-db.sh down   -> hapus container
set -euo pipefail
NAME=umkm-os-pg-test
PORT=${UMKM_DEV_DB_PORT:-55432}
RNAME=umkm-os-redis-test
RPORT=${UMKM_DEV_REDIS_PORT:-56379}
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
    # Role cluster-level dibuat SEKALI di sini: sqlx::test menjalankan migration 0007 paralel
    # di banyak DB uji; CREATE ROLE serentak pada cluster baru bisa gagal unique_violation.
    docker exec "$NAME" psql -q -U $USER -d $DB -c \
      "DO \$\$ BEGIN CREATE ROLE umkm_app_role NOLOGIN NOSUPERUSER NOBYPASSRLS; EXCEPTION WHEN duplicate_object THEN NULL; END \$\$;" >/dev/null
    if ! docker ps --format '{{.Names}}' | grep -qx "$RNAME"; then
      docker rm -f "$RNAME" >/dev/null 2>&1 || true
      docker run -d --name "$RNAME" -p "127.0.0.1:$RPORT:6379" redis:7.4-alpine \
        redis-server --save "" --appendonly no --requirepass "$PASS" >/dev/null
    fi
    for _ in $(seq 1 30); do
      docker exec -e REDISCLI_AUTH="$PASS" "$RNAME" redis-cli ping 2>/dev/null | grep -q PONG && break; sleep 0.3
    done
    echo "export DATABASE_URL=postgres://$USER:$PASS@127.0.0.1:$PORT/$DB"
    echo "export REDIS_URL=redis://:$PASS@127.0.0.1:$RPORT/0"
    ;;
  down) docker rm -f "$NAME" "$RNAME" >/dev/null && echo "removed $NAME $RNAME" ;;
  *) echo "usage: $0 up|down" >&2; exit 2 ;;
esac
