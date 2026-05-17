-- Events/audit trail and cost/session views

CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,
    agent_name TEXT,
    session_id TEXT,
    detail TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_events_type ON events(type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at DESC);

CREATE VIEW IF NOT EXISTS v_cost_summary AS
SELECT
    date(m.created_at, 'unixepoch') AS day,
    s.agent_name,
    COUNT(*) AS turns,
    COALESCE(SUM(m.tokens_in), 0) AS total_tokens_in,
    COALESCE(SUM(m.tokens_out), 0) AS total_tokens_out,
    COALESCE(SUM(m.cost_usd), 0.0) AS total_cost
FROM messages m
JOIN sessions s ON s.id = m.session_id
GROUP BY day, s.agent_name;

CREATE VIEW IF NOT EXISTS v_sessions AS
SELECT
    s.*,
    (SELECT COUNT(*) FROM messages WHERE session_id = s.id) AS turn_count,
    (SELECT COALESCE(SUM(cost_usd), 0.0) FROM messages WHERE session_id = s.id) AS session_cost,
    (SELECT COALESCE(SUM(tokens_in), 0) FROM messages WHERE session_id = s.id) AS total_tokens_in,
    (SELECT COALESCE(SUM(tokens_out), 0) FROM messages WHERE session_id = s.id) AS total_tokens_out,
    sp.key AS pointer_key,
    sp.phrase AS pointer_phrase
FROM sessions s
LEFT JOIN session_pointers sp ON sp.session_id = s.id AND sp.archived = 0;

CREATE VIEW IF NOT EXISTS v_cost_today AS
SELECT
    COALESCE(SUM(m.cost_usd), 0.0) AS total,
    COUNT(DISTINCT m.session_id) AS sessions,
    COALESCE(SUM(m.tokens_in), 0) AS tokens_in,
    COALESCE(SUM(m.tokens_out), 0) AS tokens_out
FROM messages m
WHERE date(m.created_at, 'unixepoch') = date('now');
