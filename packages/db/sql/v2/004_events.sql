CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    type        TEXT NOT NULL,
    session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    run_id      TEXT REFERENCES runs(id) ON DELETE SET NULL,
    blueprint   TEXT,
    detail      TEXT,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_events_type    ON events(type, created_at DESC);
CREATE INDEX idx_events_session ON events(session_id, id) WHERE session_id IS NOT NULL;
CREATE INDEX idx_events_recent  ON events(id DESC);
