-- Kebalikan 0042: kembalikan CHECK constraint lama (bentuk longgar) seperti sebelum migrasi.
-- PENTING: file ini hanya diuji di DB kosong (CI: run → revert → run); bukan alat rollback
-- produksi berisi data (lihat docs/governance/runbook-deploy-rollback.md §3).

-- business_profiles: kembali tanpa CHECK (bentuk dijaga di Rust).
ALTER TABLE business_profiles
    DROP CONSTRAINT IF EXISTS ck_business_profiles_phone,
    DROP CONSTRAINT IF EXISTS ck_business_profiles_whatsapp,
    DROP CONSTRAINT IF EXISTS ck_business_profiles_email;

-- customers: kembali ke bentuk longgar E.164 (seperti 0015).
ALTER TABLE customers
    DROP CONSTRAINT IF EXISTS ck_customers_email,
    DROP CONSTRAINT IF EXISTS ck_customers_phone;

ALTER TABLE customers
    ADD CONSTRAINT ck_customers_email CHECK (email IS NULL OR (length(email) <= 254 AND email = lower(btrim(email)))),
    ADD CONSTRAINT ck_customers_phone CHECK (phone IS NULL OR phone ~ '^\+[1-9][0-9]{6,14}$');

-- customer_accounts: kembali ke bentuk longgar (seperti 0015).
ALTER TABLE customer_accounts
    DROP CONSTRAINT IF EXISTS ck_customer_accounts_email;

ALTER TABLE customer_accounts
    ADD CONSTRAINT ck_customer_accounts_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%' AND email = lower(btrim(email)));

-- users & invitations: kembali ke bentuk longgar (seperti 0002).
ALTER TABLE users
    DROP CONSTRAINT IF EXISTS ck_users_email;

ALTER TABLE users
    ADD CONSTRAINT ck_users_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%' AND email = btrim(email));

ALTER TABLE invitations
    DROP CONSTRAINT IF EXISTS ck_invitations_email;

ALTER TABLE invitations
    ADD CONSTRAINT ck_invitations_email CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%');

-- orders: snapshot kontak tanpa CHECK (seperti 0016).
ALTER TABLE orders
    DROP CONSTRAINT IF EXISTS ck_orders_customer_email,
    DROP CONSTRAINT IF EXISTS ck_orders_customer_phone;
