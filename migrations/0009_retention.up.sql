-- P1-9: purge retensi (ADR-0012, PRD.md §5.2 "explicit retention dan purge policy").
-- Fungsi SECURITY DEFINER milik owner: bekerja lintas tenant & melewati append-only audit
-- secara terkendali. Role aplikasi hanya dapat EXECUTE; batas bawah keamanan di dalam
-- fungsi mencegah salah konfigurasi menghapus data terlalu baru.

CREATE FUNCTION app.purge_audit(p_before timestamptz) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n bigint;
BEGIN
    IF p_before > now() - interval '90 days' THEN
        RAISE EXCEPTION 'purge_audit: cutoff % terlalu baru (minimal 90 hari)', p_before
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM set_config('app.audit_purge', 'on', true);
    DELETE FROM public.audit_logs WHERE created_at < p_before;
    GET DIAGNOSTICS n = ROW_COUNT;
    PERFORM set_config('app.audit_purge', 'off', true);
    RETURN n;
END $$;

-- Tenant ditutup lebih lama dari grace: hapus tenant + data milik tenant (CASCADE).
-- audit_logs sengaja tidak ber-FK sehingga jejak audit tetap ada sampai purge_audit.
CREATE FUNCTION app.purge_closed_tenants(p_grace interval) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n bigint;
BEGIN
    IF p_grace < interval '30 days' THEN
        RAISE EXCEPTION 'purge_closed_tenants: grace % < 30 hari', p_grace USING ERRCODE = 'invalid_parameter_value';
    END IF;
    DELETE FROM public.tenants WHERE status = 'closed' AND closed_at < now() - p_grace;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END $$;

-- Sesi/token yang sudah tidak berlaku lebih dari p_before (minimalisasi data).
CREATE FUNCTION app.purge_auth_artifacts(p_before timestamptz)
    RETURNS TABLE (sessions bigint, tokens bigint)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE s bigint; t bigint;
BEGIN
    IF p_before > now() - interval '1 day' THEN
        RAISE EXCEPTION 'purge_auth_artifacts: cutoff terlalu baru' USING ERRCODE = 'invalid_parameter_value';
    END IF;
    DELETE FROM public.user_sessions
        WHERE expires_at < p_before OR (revoked_at IS NOT NULL AND revoked_at < p_before);
    GET DIAGNOSTICS s = ROW_COUNT;
    DELETE FROM public.user_tokens
        WHERE expires_at < p_before OR (consumed_at IS NOT NULL AND consumed_at < p_before);
    GET DIAGNOSTICS t = ROW_COUNT;
    RETURN QUERY SELECT s, t;
END $$;

-- Event outbox selesai (sent/dead). Pending/processing tidak pernah dihapus.
CREATE FUNCTION app.purge_outbox(p_before timestamptz) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path = pg_catalog, public
AS $$
DECLARE n bigint;
BEGIN
    IF p_before > now() - interval '1 day' THEN
        RAISE EXCEPTION 'purge_outbox: cutoff terlalu baru' USING ERRCODE = 'invalid_parameter_value';
    END IF;
    DELETE FROM public.notification_outbox
        WHERE status IN ('sent', 'dead') AND coalesce(processed_at, created_at) < p_before;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END $$;

REVOKE ALL ON FUNCTION app.purge_audit(timestamptz), app.purge_closed_tenants(interval),
    app.purge_auth_artifacts(timestamptz), app.purge_outbox(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.purge_audit(timestamptz), app.purge_closed_tenants(interval),
    app.purge_auth_artifacts(timestamptz), app.purge_outbox(timestamptz) TO umkm_app_role;
