-- User-visible short ID surfaced in the crash dialog and the email summary.
-- Nullable: rows written before this column existed stay NULL.
ALTER TABLE crash_reports ADD COLUMN short_id TEXT;

CREATE INDEX idx_crash_reports_short_id ON crash_reports(short_id);
