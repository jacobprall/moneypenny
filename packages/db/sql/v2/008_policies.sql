CREATE TABLE policies (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL UNIQUE,
    effect       TEXT NOT NULL CHECK (effect IN ('deny','warn','allow')),
    description  TEXT NOT NULL,
    conditions   TEXT,
    enabled      INTEGER NOT NULL DEFAULT 1,
    source_path  TEXT NOT NULL,
    updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
