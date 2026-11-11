#!/bin/sh
# Dijalankan entrypoint postgres SEKALI saat volume data baru (P1-7, PRD.md §5.7).
# Membuat login aplikasi `umkm_app`: bukan superuser, tanpa BYPASSRLS. Hak tabel diberikan
# migration 0007 lewat keanggotaan `umkm_app_role`. Password dari secret runtime.
set -eu
: "${APP_DB_PASSWORD:?APP_DB_PASSWORD wajib}"
psql -v ON_ERROR_STOP=1 -v pw="$APP_DB_PASSWORD" -v db="$POSTGRES_DB" \
     --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
CREATE ROLE umkm_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE PASSWORD :'pw';
GRANT CONNECT ON DATABASE :"db" TO umkm_app;
SQL
