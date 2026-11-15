-- P10 lanjutan: payload QRIS statis diturunkan dari gambar yang sudah diunggah.
-- Sumber kebenaran pindah ke commerce_settings; gopay_sessions.static_qris
-- tetap ada sebagai kolom legacy untuk kompatibilitas image lama.

ALTER TABLE commerce_settings
    ADD COLUMN qris_static_payload text;

ALTER TABLE commerce_settings
    ADD CONSTRAINT ck_commerce_settings_qris_payload
    CHECK (qris_static_payload IS NULL OR octet_length(qris_static_payload) BETWEEN 20 AND 1024);

UPDATE commerce_settings cs
SET qris_static_payload = gs.static_qris
FROM gopay_sessions gs
WHERE cs.tenant_id = gs.tenant_id
  AND cs.qris_static_payload IS NULL
  AND gs.static_qris IS NOT NULL;
