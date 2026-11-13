-- P7-5: agregasi platform (PRD.md §4.14, FR-ANL-002). Satu fungsi SECURITY DEFINER yang hanya mengembalikan
-- ANGKA agregat lintas tenant (tanpa nama pelanggan/isi). Otorisasi super_admin + sesi MFA ditegakkan API;
-- tenant (termasuk Owner) tidak punya jalur ke endpoint ini (AC-ANL-002).
CREATE FUNCTION app.platform_overview(p_from date, p_to date) RETURNS jsonb
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
    SELECT jsonb_build_object(
        'tenants', (SELECT jsonb_build_object(
            'active', count(*) FILTER (WHERE status = 'active'),
            'suspended', count(*) FILTER (WHERE status = 'suspended'),
            'closed', count(*) FILTER (WHERE status = 'closed'),
            'new_in_range', count(*) FILTER (WHERE app.jkt_day(created_at) BETWEEN p_from AND p_to))
            FROM tenants),
        -- Status efektif: uji coba/periode yang sudah lewat dihitung expired.
        'subscriptions', (SELECT coalesce(jsonb_agg(jsonb_build_object('plan', plan_code, 'status', st, 'count', n)
                                  ORDER BY plan_code, st), '[]'::jsonb)
            FROM (SELECT plan_code,
                         CASE WHEN status = 'trialing' AND trial_ends_at > now() THEN 'trialing'
                              WHEN status = 'active' AND current_period_end > now() THEN 'active'
                              ELSE 'expired' END AS st,
                         count(*) AS n
                    FROM subscriptions GROUP BY 1, 2) s),
        'ai', (SELECT jsonb_build_object(
            'runs', count(*),
            'prompt_tokens', coalesce(sum(prompt_tokens), 0),
            'completion_tokens', coalesce(sum(completion_tokens), 0),
            'platform_tokens', coalesce(sum(prompt_tokens + completion_tokens) FILTER (WHERE source = 'platform'), 0),
            'byok_tokens', coalesce(sum(prompt_tokens + completion_tokens) FILTER (WHERE source = 'byok'), 0),
            'provider_calls', count(*) FILTER (WHERE model <> '-' AND status NOT IN ('blocked', 'quota_exceeded', 'circuit_open')),
            'provider_errors', count(*) FILTER (WHERE status IN ('error', 'timeout')),
            'error_codes', (SELECT coalesce(jsonb_object_agg(code, n), '{}'::jsonb) FROM (
                SELECT coalesce(error_code, status) AS code, count(*) AS n FROM ai_runs
                 WHERE status IN ('error', 'timeout') AND app.jkt_day(created_at) BETWEEN p_from AND p_to
                 GROUP BY 1) e))
            FROM ai_runs WHERE app.jkt_day(created_at) BETWEEN p_from AND p_to),
        'outbox', (SELECT jsonb_build_object(
            'pending', count(*) FILTER (WHERE status IN ('pending', 'processing')),
            'dead', count(*) FILTER (WHERE status = 'dead'))
            FROM notification_outbox)
    )
$$;
REVOKE ALL ON FUNCTION app.platform_overview(date, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_overview(date, date) TO umkm_app_role;
