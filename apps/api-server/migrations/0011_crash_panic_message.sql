-- The panic payload string ("there is no reactor running", "index out of bounds: …"), which
-- turns a crash report from "something broke in this function" into a diagnosis at a glance.
-- Nullable: signal crashes (SIGSEGV/SIGBUS/SIGABRT) carry no panic payload, and rows written
-- before this column existed stay NULL.
--
-- The client redacts the message through the shared `crate::redact` pipeline (the same one the
-- error reporter runs over log lines) and caps it at 2,000 chars before it leaves the machine;
-- the ingestion endpoint caps again. So this column holds scrubbed, bounded text, never raw
-- paths or user data.
ALTER TABLE crash_reports ADD COLUMN panic_message TEXT;
