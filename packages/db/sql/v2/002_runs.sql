CREATE TABLE runs (
    id           TEXT PRIMARY KEY NOT NULL,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    status       TEXT NOT NULL CHECK (status IN ('running','complete','failed','aborted')),
    model        TEXT,
    blueprint    TEXT,
    started_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at  INTEGER,
    tokens_in    INTEGER,
    tokens_out   INTEGER,
    cost_usd     REAL,
    error        TEXT
);

CREATE INDEX idx_runs_session ON runs(session_id, started_at DESC);
