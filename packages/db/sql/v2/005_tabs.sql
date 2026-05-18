CREATE TABLE tabs (
    id          TEXT PRIMARY KEY NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('session','overview','ideas','search')),
    session_id  TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    label       TEXT,
    position    INTEGER NOT NULL,
    is_active   INTEGER NOT NULL DEFAULT 0,
    opened_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_tabs_position ON tabs(position);
CREATE UNIQUE INDEX idx_tabs_one_active ON tabs(is_active) WHERE is_active = 1;
