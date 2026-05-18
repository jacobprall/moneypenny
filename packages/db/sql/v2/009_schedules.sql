CREATE TABLE schedules (
    id              TEXT PRIMARY KEY NOT NULL,
    blueprint       TEXT NOT NULL,
    cron_expr       TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_run_at     INTEGER,
    last_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    next_run_at     INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_schedules_due ON schedules(next_run_at) WHERE enabled = 1;
