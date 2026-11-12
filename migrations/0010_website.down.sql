ALTER TABLE IF EXISTS website_pages DROP CONSTRAINT IF EXISTS fk_website_pages_published;
DROP TABLE IF EXISTS website_page_versions;
DROP TABLE IF EXISTS website_pages;
DROP TABLE IF EXISTS tenant_websites;
DROP TABLE IF EXISTS website_templates;
