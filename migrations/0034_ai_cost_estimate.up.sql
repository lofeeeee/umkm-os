-- AC-SUB-002 (FR-SUB-002): setiap panggilan AI menyimpan token input/output (sudah ada) DAN estimasi biaya.
-- Rupiah, dihitung dari harga per juta token provider platform (config operator). NULL = tidak diketahui
-- (BYOK: ditagih provider ke akun toko; platform tanpa harga terkonfigurasi).
ALTER TABLE ai_runs ADD COLUMN estimated_cost_idr numeric(14,4),
    ADD CONSTRAINT ck_ai_runs_cost CHECK (estimated_cost_idr IS NULL OR estimated_cost_idr >= 0);
