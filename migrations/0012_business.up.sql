-- P2-5: Business profile, alamat, jam operasional, exception (PRD.md §4.4, §5.4).
-- Validasi format (email, telepon, URL, timezone IANA) di Rust; DB menjaga bentuk + kepemilikan.

ALTER TABLE business_profiles
    ADD COLUMN description        text,
    ADD COLUMN logo_object_id     uuid,
    ADD COLUMN cover_object_id    uuid,
    ADD COLUMN phone              text,
    ADD COLUMN email              text,
    ADD COLUMN whatsapp           text,
    ADD COLUMN socials            jsonb   NOT NULL DEFAULT '[]',
    ADD COLUMN timezone           text    NOT NULL DEFAULT 'Asia/Jakarta',
    ADD COLUMN pickup_available   boolean NOT NULL DEFAULT false,
    ADD COLUMN delivery_available boolean NOT NULL DEFAULT false,
    ADD COLUMN fulfillment_note   text,
    ADD CONSTRAINT ck_business_profiles_description CHECK (description IS NULL OR length(description) <= 2000),
    ADD CONSTRAINT ck_business_profiles_socials CHECK (jsonb_typeof(socials) = 'array'),
    ADD CONSTRAINT ck_business_profiles_note CHECK (fulfillment_note IS NULL OR length(fulfillment_note) <= 500),
    -- Gambar wajib objek storage milik tenant yang sama (ADR-0009); tidak dapat dihapus selagi dipakai.
    ADD CONSTRAINT fk_business_profiles_logo FOREIGN KEY (tenant_id, logo_object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT fk_business_profiles_cover FOREIGN KEY (tenant_id, cover_object_id)
        REFERENCES stored_objects (tenant_id, id) ON DELETE RESTRICT;

CREATE TABLE business_addresses (
    id         uuid             NOT NULL DEFAULT gen_random_uuid(),
    tenant_id  uuid             NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    position   smallint         NOT NULL,
    label      text,
    address    text             NOT NULL,
    latitude   double precision,
    longitude  double precision,
    created_at timestamptz      NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CONSTRAINT uq_business_addresses_position UNIQUE (tenant_id, position),
    CONSTRAINT ck_business_addresses_position CHECK (position BETWEEN 0 AND 9),
    CONSTRAINT ck_business_addresses_address CHECK (length(btrim(address)) BETWEEN 1 AND 500),
    CONSTRAINT ck_business_addresses_label CHECK (label IS NULL OR length(label) <= 60),
    CONSTRAINT ck_business_addresses_coords CHECK (
        (latitude IS NULL) = (longitude IS NULL)
        AND (latitude IS NULL OR latitude BETWEEN -90 AND 90)
        AND (longitude IS NULL OR longitude BETWEEN -180 AND 180))
);

-- day_of_week ISO: 1 = Senin … 7 = Minggu. close_time < open_time = tutup lewat tengah malam.
CREATE TABLE business_hours (
    id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    day_of_week smallint    NOT NULL,
    open_time   time        NOT NULL,
    close_time  time        NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uq_business_hours_day UNIQUE (tenant_id, day_of_week),
    CONSTRAINT ck_business_hours_day CHECK (day_of_week BETWEEN 1 AND 7),
    CONSTRAINT ck_business_hours_span CHECK (open_time <> close_time)
);

CREATE TABLE business_hour_exceptions (
    id         uuid    NOT NULL DEFAULT gen_random_uuid(),
    tenant_id  uuid    NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    date       date    NOT NULL,
    is_closed  boolean NOT NULL,
    open_time  time,
    close_time time,
    note       text,
    PRIMARY KEY (id),
    CONSTRAINT uq_business_hour_exceptions_date UNIQUE (tenant_id, date),
    CONSTRAINT ck_business_hour_exceptions_hours CHECK (
        (is_closed AND open_time IS NULL AND close_time IS NULL)
        OR (NOT is_closed AND open_time IS NOT NULL AND close_time IS NOT NULL AND open_time <> close_time)),
    CONSTRAINT ck_business_hour_exceptions_note CHECK (note IS NULL OR length(note) <= 120)
);

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['business_addresses', 'business_hours', 'business_hour_exceptions'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = app.current_tenant_id()) WITH CHECK (tenant_id = app.current_tenant_id())', t);
    END LOOP;
END $$;
