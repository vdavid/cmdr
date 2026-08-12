-- Data minimization on the telemetry tables, so what we store matches what the privacy policy
-- promises. Three independent changes:
--
-- 1. `downloads.ua_family`: the install-plausibility family (`human` / `bot` / `unknown`), computed
--    at write time by `classifyUaFamily`. Until now the family was derived at QUERY time from the
--    raw `user_agent`, which forced us to keep that raw string forever. With the family in its own
--    column the signal survives the retention sweep clearing `user_agent` (migration 0014). Legacy
--    rows keep a NULL family and fall back to classifying their stored UA.
--
-- 2. `crash_reports.hashed_ip`: erased, and no longer written. Nothing ever read it (crashes group
--    by `top_function`, and by `diag_id` where present), so it was retained personal data with no
--    purpose. The column stays (NOT NULL, so rows are set to '') rather than being dropped: a D1
--    table rebuild is a worse trade than one dead column.
--
-- 3. `update_checks`: no schema change, but note the same erase does NOT apply. That table's
--    `hashed_ip` is the aggregation key and it is pruned to seven days by the cron.
--
-- The IP hashes written from here on also carry the `IP_HASH_PEPPER` secret (see `hashCallerIp`).
-- Rows written before that secret existed carry a date-only salt and are brute-forceable; the
-- retention sweep in migration 0014 clears them, and this migration erases the crash-table ones now.

ALTER TABLE downloads ADD COLUMN ua_family TEXT;

CREATE INDEX idx_downloads_ua_family ON downloads(ua_family);

UPDATE crash_reports SET hashed_ip = '' WHERE hashed_ip != '';
