DROP FUNCTION IF EXISTS app.gopay_expire_charges(integer);
DROP FUNCTION IF EXISTS app.gopay_due_tenants(integer);
ALTER TABLE payments DROP CONSTRAINT ck_payments_decided;
ALTER TABLE payments ADD CONSTRAINT ck_payments_decided
    CHECK ((status IN ('confirmed', 'rejected')) = (decided_by IS NOT NULL));
