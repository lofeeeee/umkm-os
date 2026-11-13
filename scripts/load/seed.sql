-- P8-2: dataset benchmark — :n toko (keputusan Gate H: skala 100–500 UMKM → default 500).
-- Dijalankan sebagai owner DB terhadap database KHUSUS uji beban (jangan di produksi).
-- Per toko: 1 pemilik + sesi login (token deterministik "lt-<slug>"), 20 produk/varian/stok, 50 pelanggan,
-- 100 pesanan + item (status campuran), 5 tiket, 10 percakapan × 5 pesan. Toko ganjil = Pro + chat AI (mode auto,
-- provider platform), toko genap = uji coba Starter.
\set ON_ERROR_STOP on
BEGIN;

INSERT INTO tenants (name, slug)
SELECT 'Toko Load ' || i, 'load-' || lpad(i::text, 4, '0') FROM generate_series(1, :n) i;

CREATE TEMP TABLE lt AS
SELECT id AS tenant_id, slug, (substr(slug, 6))::int AS i FROM tenants WHERE slug LIKE 'load-%';

INSERT INTO tenant_domains (tenant_id, hostname, type, status, verified_at)
SELECT tenant_id, slug || '.' || :'root', 'subdomain', 'active', now() FROM lt;
INSERT INTO business_profiles (tenant_id, name) SELECT tenant_id, 'Toko Load ' || i FROM lt;

-- Pemilik + membership + sesi (token & CSRF diekspor oleh run.sh).
INSERT INTO users (email, password_hash, email_verified_at)
SELECT slug || '@load.test', '$argon2id$load-not-a-real-hash', now() FROM lt;
INSERT INTO tenant_memberships (tenant_id, user_id, role_id)
SELECT lt.tenant_id, u.id, r.id FROM lt
JOIN users u ON u.email = lt.slug || '@load.test'
JOIN roles r ON r.tenant_id IS NULL AND r.name = 'Owner';
INSERT INTO user_sessions (user_id, token_hash, expires_at)
SELECT u.id, sha256(convert_to('lt-' || lt.slug, 'UTF8')), now() + interval '2 days'
FROM lt JOIN users u ON u.email = lt.slug || '@load.test';

-- Paket: ganjil Pro aktif.
UPDATE subscriptions s SET plan_code = 'pro', status = 'active', current_period_end = now() + interval '30 days'
FROM lt WHERE s.tenant_id = lt.tenant_id AND lt.i % 2 = 1;

-- Katalog: 20 produk aktif per toko, 1 varian, stok 1000 (baris inventory dibuat trigger varian).
INSERT INTO products (tenant_id, name, slug, status, available)
SELECT lt.tenant_id, 'Produk ' || p, 'produk-' || p, 'active', true FROM lt, generate_series(1, 20) p;
INSERT INTO product_variants (tenant_id, product_id, sku, name, price, is_active, position)
SELECT pr.tenant_id, pr.id, 'SKU-' || substr(pr.slug, 8), 'Default', 10000 + (random() * 90000)::int, true, 0
FROM products pr JOIN lt ON lt.tenant_id = pr.tenant_id;
UPDATE inventory_levels l SET on_hand = 1000, min_stock = 5 FROM lt WHERE l.tenant_id = lt.tenant_id;

INSERT INTO customers (tenant_id, name, email, created_at)
SELECT lt.tenant_id, 'Pelanggan ' || c, 'c' || c || '-' || lt.slug || '@load.test', now() - (random() * interval '60 days')
FROM lt, generate_series(1, 50) c;

-- Pesanan: dibuat AWAITING_PAYMENT (aturan state machine), status akhir di-set setelahnya.
CREATE TEMP TABLE lt_orders AS
SELECT lt.tenant_id, o AS n,
       (ARRAY['COMPLETED','COMPLETED','PROCESSING','PAID','CANCELLED','EXPIRED','AWAITING_PAYMENT'])[1 + (o % 7)] AS final_status
FROM lt, generate_series(1, 100) o;
INSERT INTO orders (tenant_id, code, customer_id, status, fulfillment, customer_name, subtotal, delivery_fee, total,
    access_token_hash, idempotency_key, request_hash, privacy_version, terms_version, consented_at, expires_at, created_at)
SELECT x.tenant_id, 'ORD-' || upper(substr(md5(x.tenant_id::text || x.n), 1, 6)),
       (SELECT id FROM customers c WHERE c.tenant_id = x.tenant_id ORDER BY c.id LIMIT 1 OFFSET (x.n % 50)),
       'AWAITING_PAYMENT', 'pickup', 'Pelanggan', 50000, 0, 50000,
       sha256(convert_to(x.tenant_id::text || x.n, 'UTF8')), x.tenant_id::text || '-' || x.n,
       sha256(convert_to('r' || x.n, 'UTF8')), 'v1-20261113', 'v1-20261113', now(), now() + interval '1 hour',
       now() - (random() * interval '30 days')
FROM lt_orders x;
INSERT INTO order_items (tenant_id, order_id, position, product_name, variant_name, sku, unit_price, quantity, line_total)
SELECT o.tenant_id, o.id, 0, 'Produk ' || (1 + abs(hashtext(o.code)) % 20), 'Default', 'SKU-1', 25000, 2, 50000
FROM orders o JOIN lt ON lt.tenant_id = o.tenant_id;
SET LOCAL session_replication_role = replica;
UPDATE orders o SET status = x.final_status
FROM lt_orders x
WHERE o.tenant_id = x.tenant_id AND o.idempotency_key = x.tenant_id::text || '-' || x.n AND x.final_status <> 'AWAITING_PAYMENT';
SET LOCAL session_replication_role = origin;

INSERT INTO tickets (tenant_id, code, source, subject, first_response_due_at, resolution_due_at)
SELECT lt.tenant_id, 'TKT-' || upper(substr(md5(lt.slug || t), 1, 10)), 'manual', 'Tiket ' || t,
       now() + interval '1 day', now() + interval '3 days'
FROM lt, generate_series(1, 5) t;

-- Percakapan historis (statistik harian ikut terisi trigger P6-6).
CREATE TEMP TABLE lt_conv AS
SELECT gen_random_uuid() AS id, lt.tenant_id FROM lt, generate_series(1, 10);
INSERT INTO conversations (id, tenant_id, consent_version, consented_at)
SELECT id, tenant_id, 'ai-consent-v1-20261113', now() FROM lt_conv;
INSERT INTO conversation_messages (tenant_id, conversation_id, sender_type, body)
SELECT c.tenant_id, c.id, CASE WHEN m % 2 = 1 THEN 'CUSTOMER' ELSE 'SYSTEM' END, 'Pesan uji ' || m
FROM lt_conv c, generate_series(1, 5) m;

-- Chat AI untuk toko Pro (ganjil): provider platform model pertama operator ("mock-1" di lingkungan uji).
INSERT INTO ai_providers (tenant_id, name, source, provider_type, model, capabilities)
SELECT tenant_id, 'Platform', 'platform', 'platform', 'mock-1', '{"chat":true,"usage_reporting":true}' FROM lt WHERE i % 2 = 1;
INSERT INTO chatbot_configs (tenant_id, enabled, mode, primary_provider_id, timeout_ms)
SELECT p.tenant_id, true, 'auto', p.id, 10000 FROM ai_providers p JOIN lt ON lt.tenant_id = p.tenant_id;

COMMIT;
ANALYZE;
