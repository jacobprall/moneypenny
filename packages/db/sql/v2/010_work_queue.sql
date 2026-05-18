CREATE TABLE work_queue (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    type          TEXT NOT NULL,
    session_id    TEXT,
    payload       TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    processed_at  INTEGER,
    error         TEXT
);

CREATE INDEX idx_work_pending ON work_queue(type, processed_at) WHERE processed_at IS NULL;

CREATE TABLE config (
    key         TEXT PRIMARY KEY NOT NULL,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
