-- Urutan menu halaman yang diatur pemilik (seret di panel Halaman Studio). NULL = belum diatur
-- (jatuh ke urutan slug). Beranda selalu pertama, apa pun nilainya.
ALTER TABLE website_pages ADD COLUMN nav_position integer;
ALTER TABLE website_pages ADD CONSTRAINT ck_website_pages_nav_position
    CHECK (nav_position IS NULL OR nav_position BETWEEN 0 AND 1000);
