-- Where installs come from.
--
-- Per beat, on `heartbeat`:
-- `country`: the ISO-2 country Cloudflare derived from the request (`request.cf.country`). NULL when
-- Cloudflare had none or sent a non-country code (`XX` unknown, `T1` Tor). Derived per request; the
-- IP is still never stored.
-- `install_channel`: `homebrew` | `direct`, how the app was installed. NULL from older clients.
ALTER TABLE heartbeat ADD COLUMN country TEXT;
ALTER TABLE heartbeat ADD COLUMN install_channel TEXT;

-- One row per install, kept in step with `heartbeat` by `/heartbeat` (an upsert in the beat's own
-- batch) and by the two sweeps that delete beats. It exists so the funnel's per-install facts (first
-- beat, country, channel) read a table of installs rather than scanning every beat on every
-- dashboard load: `apps/api-server/src/admin/DETAILS.md` § Install summary.
-- `first_beat_at`: the install's first-ever beat, the same value as that beat's `created_at`.
-- `country` / `install_channel`: the first non-NULL value the install ever reported, so an install
-- that predates the field gains one on its next beat.
CREATE TABLE install_summary (
    anal_id TEXT PRIMARY KEY,
    first_beat_at TEXT NOT NULL,
    country TEXT,
    install_channel TEXT
);

CREATE INDEX idx_install_summary_first_beat ON install_summary(first_beat_at);

-- Retention joins a cohort's installs to their beats in a one-week window. Keyed on the install alone,
-- every such lookup reads that install's whole history; with `created_at` beside it, only the window.
-- It also serves everything `idx_heartbeat_anal` did (the synthetic sweep's `GROUP BY anal_id`), so
-- that one goes rather than costing a second index write on every beat.
CREATE INDEX idx_heartbeat_anal_created ON heartbeat(anal_id, created_at);
DROP INDEX idx_heartbeat_anal;

-- Every install already in the table, with its first beat. Country and channel stay NULL: no earlier
-- beat carried them.
INSERT INTO install_summary (anal_id, first_beat_at)
    SELECT anal_id, MIN(created_at) FROM heartbeat GROUP BY anal_id;
