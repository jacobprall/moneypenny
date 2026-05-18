CREATE TABLE skills (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL UNIQUE,
    description       TEXT NOT NULL,
    instructions      TEXT,
    confidence        REAL NOT NULL DEFAULT 0.5,
    source_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE conventions (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL UNIQUE,
    category          TEXT NOT NULL,
    description       TEXT NOT NULL,
    confidence        REAL NOT NULL DEFAULT 0.5,
    source_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE session_pointers (
    id          TEXT PRIMARY KEY NOT NULL,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    phrase      TEXT NOT NULL,
    pinned      INTEGER NOT NULL DEFAULT 0,
    archived    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_pointers_session ON session_pointers(session_id);
CREATE INDEX idx_pointers_active  ON session_pointers(pinned DESC, created_at DESC) WHERE archived = 0;
