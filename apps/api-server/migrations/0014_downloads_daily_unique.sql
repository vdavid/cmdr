-- Per-day distinct-downloader counts, captured by the retention sweep just before it clears the
-- `downloads.hashed_ip` values those counts are derived from (`handleRetentionSweep`).
--
-- Same shape of trade as `daily_active_users`: the identifying column has a bounded life, the number
-- computed from it does not. `/admin/downloads` reads this rollup for any day whose hashes are gone
-- and the live `COUNT(DISTINCT hashed_ip)` for days still inside the window, so the unique-downloader
-- series stays complete across the retention boundary.
--
-- The grouping MUST match what `/admin/downloads` groups by (`date`, `app_version`, `arch`, `country`,
-- `source`), because a distinct count is not additive: it can't be re-derived at a coarser grouping
-- later. `source` is stored already-coalesced to 'other', matching the query.

CREATE TABLE downloads_daily_unique (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    app_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    country TEXT NOT NULL,
    source TEXT NOT NULL,
    unique_downloaders INTEGER NOT NULL,
    UNIQUE(date, app_version, arch, country, source)
);

CREATE INDEX idx_downloads_daily_unique_date ON downloads_daily_unique(date);
