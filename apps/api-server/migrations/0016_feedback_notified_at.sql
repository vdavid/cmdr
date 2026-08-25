-- When the feedback digest email that carried this row went out, mirroring `crash_reports.notified_at`.
-- NULL means the cron job (`handleFeedbackNotifications`) still owes David an email about it.
ALTER TABLE feedback ADD COLUMN notified_at TEXT;

-- Stamp every row that already exists as already-notified. This is load-bearing, not cleanup: without
-- it the first cron tick after this migration mails years of backlog in one digest, which David
-- explicitly does not want. Those messages have already been read in Discord and in D1.
UPDATE feedback SET notified_at = datetime('now') WHERE notified_at IS NULL;
