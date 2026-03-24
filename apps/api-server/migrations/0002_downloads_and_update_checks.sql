CREATE TABLE downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    app_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    country TEXT NOT NULL,
    continent TEXT NOT NULL
);

CREATE INDEX idx_downloads_created ON downloads(created_at);

-- Raw update checks with UNIQUE constraint for per-day deduplication.
-- Each unique (date, hashed_ip, version, arch) combo = one row.
-- INSERT OR IGNORE handles duplicates at zero cost.
CREATE TABLE update_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    hashed_ip TEXT NOT NULL,
    app_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    UNIQUE(date, hashed_ip, app_version, arch)
);

CREATE INDEX idx_update_checks_date ON update_checks(date);

-- Aggregated daily active users, computed from update_checks by the cron.
CREATE TABLE daily_active_users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    app_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    unique_users INTEGER NOT NULL,
    UNIQUE(date, app_version, arch)
);

CREATE INDEX idx_dau_date ON daily_active_users(date);
