-- Core tables: sessions, messages, config, work queue

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    label TEXT,
    agent_name TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    last_active_at INTEGER NOT NULL DEFAULT (unixepoch()),
    is_active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_sessions_agent ON sessions(agent_name);
CREATE INDEX IF NOT EXISTS idx_sessions_active ON sessions(is_active, last_active_at DESC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    turn INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT,
    tool_calls TEXT,
    tool_call_id TEXT,
    tokens_in INTEGER,
    tokens_out INTEGER,
    cost_usd REAL,
    session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, turn);

-- FTS for message search
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content, content=messages, content_rowid=rowid
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
    INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;

CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Lightweight work queue: triggers flag work, app processes it
CREATE TABLE IF NOT EXISTS work_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,          -- 'label', 'summarize', 'compress', 'consolidate'
    session_id TEXT,
    payload TEXT,                -- optional JSON context
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    processed_at INTEGER,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_work_queue_pending ON work_queue(type, processed_at)
    WHERE processed_at IS NULL;
