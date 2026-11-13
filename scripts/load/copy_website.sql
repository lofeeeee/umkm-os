-- P8-2: salin website terbit milik load-0001 (dibuat lewat API asli) ke semua toko load-* lain.
\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE src AS
SELECT w.id AS website_id, w.template_id, w.status, t.id AS tenant_id
FROM tenant_websites w JOIN tenants t ON t.id = w.tenant_id WHERE t.slug = 'load-0001';
CREATE TEMP TABLE dst AS
SELECT t.id AS tenant_id, gen_random_uuid() AS website_id FROM tenants t
WHERE t.slug LIKE 'load-%' AND t.slug <> 'load-0001'
  AND NOT EXISTS (SELECT 1 FROM tenant_websites w WHERE w.tenant_id = t.id);
INSERT INTO tenant_websites (id, tenant_id, template_id, status)
SELECT d.website_id, d.tenant_id, s.template_id, s.status FROM dst d, src s;
CREATE TEMP TABLE pmap AS
SELECT d.tenant_id, d.website_id, p.id AS src_page, gen_random_uuid() AS page_id, p.published_version_id AS src_ver
FROM dst d, src s JOIN website_pages p ON p.tenant_id = s.tenant_id AND p.website_id = s.website_id;
INSERT INTO website_pages (id, tenant_id, website_id, slug, title, draft_content, draft_revision)
SELECT m.page_id, m.tenant_id, m.website_id, p.slug, p.title, p.draft_content, p.draft_revision
FROM pmap m JOIN website_pages p ON p.id = m.src_page;
CREATE TEMP TABLE vmap AS
SELECT m.tenant_id, m.page_id, v.id AS src_ver, gen_random_uuid() AS ver_id, v.version, v.content_json
FROM pmap m JOIN website_page_versions v ON v.page_id = m.src_page;
INSERT INTO website_page_versions (id, tenant_id, page_id, version, content_json)
SELECT ver_id, tenant_id, page_id, version, content_json FROM vmap;
UPDATE website_pages p SET published_version_id = v.ver_id
FROM pmap m JOIN vmap v ON v.page_id = m.page_id AND v.src_ver = m.src_ver
WHERE p.id = m.page_id;
COMMIT;
ANALYZE;
