-- Kebalikan 0043. Hanya diuji di DB kosong (lihat docs/governance/runbook-deploy-rollback.md §3).
ALTER TABLE website_pages DROP CONSTRAINT IF EXISTS ck_website_pages_nav_position;
ALTER TABLE website_pages DROP COLUMN IF EXISTS nav_position;
