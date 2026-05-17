-- Session pointers: the "previous sessions" list

CREATE TABLE IF NOT EXISTS session_pointers (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    phrase TEXT NOT NULL,
    summary TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    pinned INTEGER NOT NULL DEFAULT 0,
    consolidated_from TEXT,     -- JSON array of source session_ids
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sp_session ON session_pointers(session_id);
CREATE INDEX IF NOT EXISTS idx_sp_key ON session_pointers(key);
CREATE INDEX IF NOT EXISTS idx_sp_active ON session_pointers(archived, pinned, created_at DESC);

-- When a session closes, queue it for summarization
CREATE TRIGGER IF NOT EXISTS trg_session_close
AFTER UPDATE OF is_active ON sessions
WHEN NEW.is_active = 0 AND OLD.is_active = 1
BEGIN
    INSERT INTO work_queue (type, session_id)
    SELECT 'summarize', NEW.id
    WHERE NOT EXISTS (
        SELECT 1 FROM session_pointers WHERE session_id = NEW.id
    );
END;
