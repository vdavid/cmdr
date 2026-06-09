-- True daily-active tracking for the open beta. One row per heartbeat (launch + hourly),
-- keyed by the random `anal_<uuid>` analytics id. Raw rows are kept forever (no prune);
-- DAU is a query-time `COUNT(DISTINCT anal_id)`, engagement is `COUNT(*)` per day, so there's
-- deliberately no UNIQUE/dedup constraint. No IP is stored: the `anal_id` is the only identity.
CREATE TABLE heartbeat (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anal_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    app_version TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    build_mode TEXT,                 -- 'release' | 'debug', nullable
    config_json TEXT                 -- allowlisted config-shape snapshot, a single JSON blob (not per-field columns)
);

CREATE INDEX idx_heartbeat_created ON heartbeat(created_at);
CREATE INDEX idx_heartbeat_anal ON heartbeat(anal_id);
