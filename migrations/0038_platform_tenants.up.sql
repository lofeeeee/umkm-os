-- P11 (pengawasan operator): dashboard admin platform dapat menelusuri UMKM per toko, termasuk
-- omzet (GMV) dan volume transaksi. BERBEDA dari app.platform_overview (yang sengaja hanya angka
-- agregat tanpa identitas): di sini operator sengaja boleh melihat nama/slug + omzet tiap toko
-- (keputusan pemilik platform). Tetap SECURITY DEFINER sempit + diaudit sebagai akses 'platform'.
--
-- Omzet memakai status pendapatan yang sama dengan analitik per-toko (REVENUE_STATUSES):
-- PAID, APPROVAL_PENDING, APPROVED, PROCESSING, READY_FOR_PICKUP, SHIPPED, COMPLETED.

-- Daftar toko dengan metrik ringkas (pagination + pencarian nama/slug).
CREATE FUNCTION app.platform_tenants(
    p_from date, p_to date, p_q text, p_limit integer, p_offset integer
) RETURNS jsonb
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    WITH base AS (
        SELECT t.id, t.name, t.slug, t.status AS tenant_status, t.created_at,
               s.plan_code,
               CASE WHEN s.status IS NULL THEN NULL
                    WHEN s.status = 'trialing' AND s.trial_ends_at > now() THEN 'trialing'
                    WHEN s.status = 'active' AND s.current_period_end > now() THEN 'active'
                    ELSE 'expired' END AS sub_status
        FROM tenants t
        LEFT JOIN subscriptions s ON s.tenant_id = t.id
        WHERE coalesce(p_q, '') = ''
           OR t.name ILIKE '%' || p_q || '%'
           OR t.slug ILIKE '%' || p_q || '%'
    ),
    agg AS (
        SELECT b.*,
               coalesce(o.orders, 0) AS orders,
               coalesce(o.paid_orders, 0) AS paid_orders,
               coalesce(o.gmv, 0) AS gmv_num,
               (SELECT max(created_at) FROM orders o2 WHERE o2.tenant_id = b.id) AS last_order_at
        FROM base b
        LEFT JOIN LATERAL (
            SELECT count(*) AS orders,
                   count(*) FILTER (WHERE status = ANY (ARRAY['PAID','APPROVAL_PENDING','APPROVED','PROCESSING','READY_FOR_PICKUP','SHIPPED','COMPLETED'])) AS paid_orders,
                   sum(total) FILTER (WHERE status = ANY (ARRAY['PAID','APPROVAL_PENDING','APPROVED','PROCESSING','READY_FOR_PICKUP','SHIPPED','COMPLETED'])) AS gmv
            FROM orders o
            WHERE o.tenant_id = b.id AND app.jkt_day(o.created_at) BETWEEN p_from AND p_to
        ) o ON true
    )
    SELECT jsonb_build_object(
        'total', (SELECT count(*) FROM base),
        'items', (
            SELECT coalesce(jsonb_agg(jsonb_build_object(
                'id', id, 'name', name, 'slug', slug, 'tenant_status', tenant_status,
                'plan', plan_code, 'sub_status', sub_status, 'created_at', created_at,
                'orders', orders, 'paid_orders', paid_orders, 'gmv', gmv_num::text,
                'last_order_at', last_order_at) ORDER BY gmv_num DESC, created_at DESC), '[]'::jsonb)
            FROM (
                SELECT * FROM agg
                ORDER BY gmv_num DESC, created_at DESC
                LIMIT greatest(1, least(p_limit, 100)) OFFSET greatest(0, p_offset)
            ) page)
    );
$$;
REVOKE ALL ON FUNCTION app.platform_tenants(date, date, text, integer, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_tenants(date, date, text, integer, integer) TO umkm_app_role;

-- Detail satu toko: metrik lebih dalam untuk drill-down.
CREATE FUNCTION app.platform_tenant_detail(p_tenant uuid, p_from date, p_to date) RETURNS jsonb
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT CASE WHEN t.id IS NULL THEN NULL ELSE jsonb_build_object(
        'id', t.id, 'name', t.name, 'slug', t.slug, 'tenant_status', t.status,
        'created_at', t.created_at,
        'primary_hostname', (SELECT hostname FROM tenant_domains d WHERE d.tenant_id = t.id
                             ORDER BY d.verified_at NULLS LAST, d.created_at LIMIT 1),
        'plan', s.plan_code,
        'sub_status', CASE WHEN s.status IS NULL THEN NULL
                           WHEN s.status = 'trialing' AND s.trial_ends_at > now() THEN 'trialing'
                           WHEN s.status = 'active' AND s.current_period_end > now() THEN 'active'
                           ELSE 'expired' END,
        'current_period_end', s.current_period_end,
        'products_active', (SELECT count(*) FROM products p WHERE p.tenant_id = t.id AND p.status = 'active'),
        'customers_total', (SELECT count(*) FROM customers c WHERE c.tenant_id = t.id),
        'last_order_at', (SELECT max(created_at) FROM orders o WHERE o.tenant_id = t.id),
        'range', (
            SELECT jsonb_build_object(
                'orders', count(*),
                'paid_orders', count(*) FILTER (WHERE status = ANY (ARRAY['PAID','APPROVAL_PENDING','APPROVED','PROCESSING','READY_FOR_PICKUP','SHIPPED','COMPLETED'])),
                'gmv', coalesce(sum(total) FILTER (WHERE status = ANY (ARRAY['PAID','APPROVAL_PENDING','APPROVED','PROCESSING','READY_FOR_PICKUP','SHIPPED','COMPLETED'])), 0)::text,
                'by_status', coalesce((SELECT jsonb_object_agg(status, n) FROM (
                    SELECT status, count(*) AS n FROM orders o2
                    WHERE o2.tenant_id = t.id AND app.jkt_day(o2.created_at) BETWEEN p_from AND p_to
                    GROUP BY status) bs), '{}'::jsonb))
            FROM orders o WHERE o.tenant_id = t.id AND app.jkt_day(o.created_at) BETWEEN p_from AND p_to
        )
    ) END
    FROM (SELECT 1) one
    LEFT JOIN tenants t ON t.id = p_tenant
    LEFT JOIN subscriptions s ON s.tenant_id = t.id;
$$;
REVOKE ALL ON FUNCTION app.platform_tenant_detail(uuid, date, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_tenant_detail(uuid, date, date) TO umkm_app_role;
