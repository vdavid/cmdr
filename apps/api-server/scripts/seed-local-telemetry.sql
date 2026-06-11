-- Sample telemetry for local QA of the analytics dashboard (downloads, update activity, heartbeat DAU).
-- Dates are relative to `now`, so the data always lands in the last few days regardless of when you run it.
-- Apply to a LOCAL D1 only:
--   wrangler d1 migrations apply cmdr-telemetry --local
--   wrangler d1 execute cmdr-telemetry --local --file=scripts/seed-local-telemetry.sql
-- See apps/analytics-dashboard/CLAUDE.md § "Local QA against a local worker".

-- ===== downloads: last 3 days, varied source, with a same-day duplicate IP to show dedup =====
INSERT INTO downloads (created_at, app_version, arch, country, continent, hashed_ip, source) VALUES
 (datetime('now','-2 days'),'0.25.0','aarch64','US','NA','1111111111111111111111111111111111111111111111111111111111111111','website'),
 (datetime('now','-2 days'),'0.25.0','aarch64','DE','EU','2222222222222222222222222222222222222222222222222222222222222222','website'),
 (datetime('now','-2 days'),'0.25.0','aarch64','DE','EU','2222222222222222222222222222222222222222222222222222222222222222','website'), -- dup IP same day: raw +1, unique unchanged
 (datetime('now','-2 days'),'0.25.0','universal','GB','EU','3333333333333333333333333333333333333333333333333333333333333333','homebrew'),
 (datetime('now','-2 days'),'0.25.0','x86_64','FR','EU','4444444444444444444444444444444444444444444444444444444444444444','other'),
 (datetime('now','-1 days'),'0.25.0','aarch64','US','NA','5555555555555555555555555555555555555555555555555555555555555555','website'),
 (datetime('now','-1 days'),'0.25.0','aarch64','SE','EU','6666666666666666666666666666666666666666666666666666666666666666','website'),
 (datetime('now','-1 days'),'0.25.0','universal','CA','NA','7777777777777777777777777777777777777777777777777777777777777777','homebrew'),
 (datetime('now','-1 days'),'0.25.0','universal','US','NA','8888888888888888888888888888888888888888888888888888888888888888','homebrew'),
 (datetime('now','-1 days'),'0.25.0','x86_64','NL','EU','9999999999999999999999999999999999999999999999999999999999999999','other'),
 (datetime('now'),'0.25.0','aarch64','US','NA','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','website'),
 (datetime('now'),'0.25.0','aarch64','JP','AS','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','website'),
 (datetime('now'),'0.25.0','universal','US','NA','dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd','homebrew'),
 (datetime('now'),'0.25.0','x86_64','BR','SA','eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee','other');
-- Pre-migration-style row: NULL source + NULL hashed_ip -> shows as 'other', excluded from the deduped count.
INSERT INTO downloads (created_at, app_version, arch, country, continent) VALUES
 (datetime('now','-3 days'),'0.24.0','aarch64','US','NA');

-- ===== update_checks: TODAY only (the cron prunes raw rows older than 7 days), mixed versions =====
INSERT OR IGNORE INTO update_checks (date, hashed_ip, app_version, arch) VALUES
 (date('now'),'f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1','0.24.0','aarch64'),
 (date('now'),'f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2','0.24.0','aarch64'),
 (date('now'),'f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3','0.25.0','aarch64'),
 (date('now'),'f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4','0.25.0','universal'),
 (date('now'),'f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5','0.25.0','x86_64');

-- ===== daily_active_users: PAST days (the retained aggregate), rollout 0.24 -> 0.25 =====
INSERT OR IGNORE INTO daily_active_users (date, app_version, arch, unique_users) VALUES
 (date('now','-4 days'),'0.24.0','aarch64',7),
 (date('now','-4 days'),'0.25.0','aarch64',4),
 (date('now','-3 days'),'0.24.0','aarch64',5),
 (date('now','-3 days'),'0.25.0','aarch64',8),
 (date('now','-2 days'),'0.24.0','aarch64',3),
 (date('now','-2 days'),'0.25.0','aarch64',11),
 (date('now','-1 days'),'0.24.0','aarch64',2),
 (date('now','-1 days'),'0.25.0','aarch64',14);

-- ===== heartbeat: a few distinct installs per day so the DAU chart fills =====
INSERT INTO heartbeat (anal_id, created_at, app_version, os_version, arch) VALUES
 ('anal_a',datetime('now','-2 days'),'0.25.0','15.5','aarch64'),
 ('anal_b',datetime('now','-2 days'),'0.25.0','15.5','aarch64'),
 ('anal_b',datetime('now','-1 days'),'0.25.0','15.5','aarch64'),
 ('anal_c',datetime('now','-1 days'),'0.25.0','15.5','aarch64'),
 ('anal_d',datetime('now','-1 days'),'0.25.0','15.5','aarch64'),
 ('anal_b',datetime('now'),'0.25.0','15.5','aarch64'),
 ('anal_c',datetime('now'),'0.25.0','15.5','aarch64'),
 ('anal_e',datetime('now'),'0.25.0','15.5','aarch64');
