CREATE TABLE sessions (
    id              TEXT PRIMARY KEY NOT NULL,
    label           TEXT,
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active','running','paused','completed','failed','archived')),
    parent_id       TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    idea_id         TEXT,
    config          TEXT NOT NULL DEFAULT '{}',
    config_version  INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    last_active_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    completed_at    INTEGER,
    failed_at       INTEGER,
    archived_at     INTEGER
);

CREATE INDEX idx_sessions_status ON sessions(status, last_active_at DESC);
CREATE INDEX idx_sessions_parent ON sessions(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_sessions_idea   ON sessions(idea_id) WHERE idea_id IS NOT NULL;

CREATE VIRTUAL TABLE sessions_fts USING fts5(label, content=sessions, content_rowid=rowid);

CREATE TRIGGER sessions_ai AFTER INSERT ON sessions BEGIN
  INSERT INTO sessions_fts(rowid, label) VALUES (NEW.rowid, NEW.label);
END;
CREATE TRIGGER sessions_ad AFTER DELETE ON sessions BEGIN
  INSERT INTO sessions_fts(sessions_fts, rowid, label) VALUES ('delete', OLD.rowid, OLD.label);
END;
CREATE TRIGGER sessions_au AFTER UPDATE OF label ON sessions BEGIN
  INSERT INTO sessions_fts(sessions_fts, rowid, label) VALUES ('delete', OLD.rowid, OLD.label);
  INSERT INTO sessions_fts(rowid, label) VALUES (NEW.rowid, NEW.label);
END;
