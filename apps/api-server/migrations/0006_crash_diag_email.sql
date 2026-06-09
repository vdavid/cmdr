-- Diagnostics id (`diag_<uuid>`) groups sequential crash reports from one install, and the
-- optional contact email a beta tester can attach at send time so we can reply about the bug.
-- Both nullable: rows written before these columns existed (and reports without an attached
-- email) stay NULL. The `diag_` id is deliberately separate from the `anal_` analytics id, so
-- a voluntarily attached email never joins to the analytics stream.
ALTER TABLE crash_reports ADD COLUMN diag_id TEXT;
ALTER TABLE crash_reports ADD COLUMN email TEXT;
