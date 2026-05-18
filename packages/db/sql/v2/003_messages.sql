CREATE TABLE messages (
    id            TEXT PRIMARY KEY NOT NULL,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    run_id        TEXT REFERENCES runs(id) ON DELETE SET NULL,
    seq           INTEGER NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('user','assistant','system','tool')),
    content       TEXT,
    tool_calls    TEXT,
    tool_call_id  TEXT,
    pending       INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_messages_session ON messages(session_id, seq);
CREATE INDEX idx_messages_run     ON messages(run_id) WHERE run_id IS NOT NULL;
CREATE INDEX idx_messages_pending ON messages(session_id, seq) WHERE pending = 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_session_seq ON messages(session_id, seq);

CREATE VIRTUAL TABLE messages_fts USING fts5(content, content=messages, content_rowid=rowid);

CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
  INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;
