-- Deterministic seed for QA of GET /admin/funnel against a LOCAL D1.
-- Dates are relative to `now`, so the data always lands in the funnel window.
-- Apply to a LOCAL D1 only (after migrations):
--   wrangler d1 migrations apply cmdr-telemetry --local
--   wrangler d1 execute cmdr-telemetry --local --file=scripts/seed-funnel-local.sql
--
-- Hand-computed expectations (with today = D0, all UTC):
--
-- New installs (first-ever heartbeat day):
--   D-9: anal_x, anal_y        -> 2
--   D-8: anal_z                 -> 1
--   D-1: anal_w                 -> 1
--   D0 : anal_today             -> 1
--
-- D7 retention (cohort day C is "knowable" only when C is >= 8 days before today):
--   D-9 cohort (knowable): installs = 2 (x, y). Retained = anal_x beats on D-2 (= D-9 + 7) -> 1.
--       anal_y never beats again -> not retained. d7Retained = 1, d7Retention = 0.5.
--   D-8 cohort (knowable, exactly 8 days old): installs = 1 (z). anal_z beats on D-1 (= D-8 + 7) -> retained.
--       d7Retained = 1, d7Retention = 1.0.
--   D-1 cohort (NOT knowable, 1 day old): installs = 1 -> d7Retention = null.
--   D0  cohort (NOT knowable): installs = 1 -> d7Retention = null.
--
-- DAU (distinct anal_id beating that day):
--   D-9: x, y          -> 2
--   D-8: z             -> 1
--   D-2: x             -> 1   (x's D7 beat)
--   D-1: z, w          -> 2   (z's D7 beat + w's install)
--   D0 : anal_today    -> 1
--
-- Downloads by source (server-side DMG fetches):
--   D-2: website 2, homebrew 1   -> 3
--   D0 : website 1, other 1      -> 2
--
-- Downloads by ref (first-touch channel; NULL ref COALESCEs to the "(none)" bucket):
--   D-2: hn 1, reddit 1, (none) 1   (the two website rows are hn + reddit; the homebrew row has no ref)
--   D0 : hn 1, (none) 1             (the website row is hn; the "other" row has no ref)

INSERT INTO heartbeat (anal_id, created_at, app_version, os_version, arch) VALUES
 -- D-9 cohort: x retained at D7, y not
 ('anal_x', datetime('now','-9 days'), '0.25.0','15.5','aarch64'),
 ('anal_y', datetime('now','-9 days'), '0.25.0','15.5','aarch64'),
 ('anal_x', datetime('now','-2 days'), '0.25.0','15.5','aarch64'),  -- D-9 + 7 = D-2, in [first+7, first+8) -> retained
 -- D-8 cohort: z retained at D7
 ('anal_z', datetime('now','-8 days'), '0.25.0','15.5','aarch64'),
 ('anal_z', datetime('now','-1 days'), '0.25.0','15.5','aarch64'),  -- D-8 + 7 = D-1 -> retained
 -- D-1 cohort: w (too young for D7)
 ('anal_w', datetime('now','-1 days'), '0.25.0','15.5','aarch64'),
 -- D0 cohort: today (too young)
 ('anal_today', datetime('now'), '0.25.0','15.5','aarch64');

INSERT INTO downloads (created_at, app_version, arch, country, continent, hashed_ip, source, ref) VALUES
 (datetime('now','-2 days'),'0.25.0','aarch64','US','NA','a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1','website','hn'),
 (datetime('now','-2 days'),'0.25.0','aarch64','DE','EU','a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2','website','reddit'),
 (datetime('now','-2 days'),'0.25.0','universal','GB','EU','a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3','homebrew',NULL),
 (datetime('now'),'0.25.0','aarch64','US','NA','a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4','website','hn'),
 (datetime('now'),'0.25.0','x86_64','BR','SA','a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5','other',NULL);
