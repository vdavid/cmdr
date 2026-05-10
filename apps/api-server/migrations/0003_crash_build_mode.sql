-- Distinguish dev-build crashes from production crashes in the email summary.
-- Nullable: rows written before this column existed stay NULL.
ALTER TABLE crash_reports ADD COLUMN build_mode TEXT;
