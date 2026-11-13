ALTER TABLE ai_runs DROP CONSTRAINT IF EXISTS ck_ai_runs_cost, DROP COLUMN IF EXISTS estimated_cost_idr;
