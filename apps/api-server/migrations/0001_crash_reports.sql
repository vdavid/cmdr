CREATE TABLE crash_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    notified_at TEXT,
    hashed_ip TEXT NOT NULL,
    app_version TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    signal TEXT NOT NULL,
    top_function TEXT NOT NULL,
    backtrace TEXT NOT NULL
);

CREATE INDEX idx_crash_reports_notified ON crash_reports(notified_at);
CREATE INDEX idx_crash_reports_created ON crash_reports(created_at);
