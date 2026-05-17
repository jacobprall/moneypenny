-- Agent definitions and scheduled jobs

CREATE TABLE IF NOT EXISTS agent_defs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    model TEXT,
    system_prompt TEXT,
    tools TEXT,               -- JSON array of tool names
    trigger_on TEXT,          -- 'manual', 'session_close', 'schedule'
    source_path TEXT,         -- .adams/ file path for hot-reload
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    schedule TEXT,            -- cron expression
    agent_name TEXT REFERENCES agent_defs(name) ON DELETE CASCADE,
    action TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at INTEGER,
    source_path TEXT,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_jobs_agent ON jobs(agent_name);
CREATE INDEX IF NOT EXISTS idx_jobs_enabled ON jobs(enabled, schedule);
