-- The `ERR-XXXXX` of the logs a feedback sender chose to include ("Include the past five hours of logs"). The app
-- uploads them as an ordinary error report under this id right after the feedback lands, and the cron files the two
-- as ONE card (`report-outbox.ts`). NULL when no logs were included. Never a foreign key: the report can arrive late or
-- not at all, and its row is deleted at the bundle's 90 days while the feedback row stays.
ALTER TABLE feedback ADD COLUMN error_report_id TEXT;
CREATE INDEX idx_feedback_error_report_id ON feedback(error_report_id) WHERE error_report_id IS NOT NULL;
