-- In-app "Send feedback" messages (open beta). One row per submission, written by
-- POST /feedback. This table is the durable sink: the Discord notification only carries
-- a truncated preview. `email` is the optional reply-to address the sender chose to
-- attach; no install id of any kind is stored, so feedback can't be joined to the
-- analytics stream.
CREATE TABLE feedback (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    feedback TEXT NOT NULL,
    email TEXT,                      -- optional reply-to, nullable
    app_version TEXT NOT NULL,
    os_version TEXT NOT NULL,
    build_mode TEXT                  -- 'release' | 'debug', nullable
);

CREATE INDEX idx_feedback_created ON feedback(created_at);
