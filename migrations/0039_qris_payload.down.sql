ALTER TABLE commerce_settings
    DROP CONSTRAINT IF EXISTS ck_commerce_settings_qris_payload;
ALTER TABLE commerce_settings
    DROP COLUMN IF EXISTS qris_static_payload;
