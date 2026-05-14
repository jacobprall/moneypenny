export const SCHEMA_VERSION = 7;
export const WORKSPACE_SCHEMA_VERSION = 1;

export interface Migration {
  version: number;
  sql: string;
}

/**
 * Ordered list of session-DB migrations.
 *
 * IMPORTANT: When adding a migration, also update SESSION_SCHEMA_SQL below.
 */
export const MIGRATIONS: Migration[] = [
  {
    version: 2,
    sql: `
CREATE TRIGGER IF NOT EXISTS code_chunks_au AFTER UPDATE ON code_chunks BEGIN
  INSERT INTO code_fts(code_fts, rowid, path, chunk_text, language)
  VALUES ('delete', old.rowid, old.path, old.chunk_text, old.language);
  INSERT INTO code_fts(rowid, path, chunk_text, language)
  VALUES (new.rowid, new.path, new.chunk_text, new.language);
END;
`,
  },
  {
    version: 3,
    sql: `
CREATE TABLE IF NOT EXISTS skills (
  name TEXT PRIMARY KEY,
  description TEXT NOT NULL,
  instructions TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'builtin',
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS subagent_defs (
  name TEXT PRIMARY KEY,
  skill TEXT NOT NULL REFERENCES skills(name),
  description TEXT NOT NULL,
  allowed_tools TEXT NOT NULL,
  max_iterations INTEGER DEFAULT 10,
  max_cost_usd REAL,
  source TEXT NOT NULL DEFAULT 'builtin',
  created_at INTEGER NOT NULL
);
`,
  },
  {
    version: 4,
    sql: `
CREATE TABLE IF NOT EXISTS skill_files (
  skill_name TEXT NOT NULL,
  path TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (skill_name, path),
  FOREIGN KEY (skill_name) REFERENCES skills(name) ON DELETE CASCADE
);
`,
  },
  {
    version: 5,
    sql: `
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  label TEXT,
  created_at INTEGER NOT NULL,
  last_active_at INTEGER NOT NULL,
  is_active INTEGER NOT NULL DEFAULT 1
);

ALTER TABLE messages ADD COLUMN session_id TEXT REFERENCES sessions(id);
ALTER TABLE events ADD COLUMN session_id TEXT REFERENCES sessions(id);
ALTER TABLE metrics ADD COLUMN session_id TEXT REFERENCES sessions(id);
ALTER TABLE compaction_markers ADD COLUMN session_id TEXT REFERENCES sessions(id);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, turn);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_metrics_session ON metrics(session_id, turn);
`,
  },
  {
    version: 6,
    sql: `
-- Governance: policies
CREATE TABLE IF NOT EXISTS policies (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  effect TEXT NOT NULL CHECK(effect IN ('allow','deny','audit','confirm')),
  priority INTEGER DEFAULT 0,
  tool_pattern TEXT,
  path_pattern TEXT,
  cost_condition TEXT,
  args_pattern TEXT,
  actor_pattern TEXT,
  message TEXT,
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Governance: hooks
CREATE TABLE IF NOT EXISTS hooks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  phase TEXT NOT NULL CHECK(phase IN ('pre:validation','pre:injection','post:transform')),
  match_pattern TEXT NOT NULL,
  priority INTEGER DEFAULT 0,
  script TEXT NOT NULL,
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Cloud sync config
CREATE TABLE IF NOT EXISTS sync_config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Agents (for platform layer)
CREATE TABLE IF NOT EXISTS agents (
  id TEXT PRIMARY KEY,
  dir_path TEXT NOT NULL,
  agent_md_path TEXT NOT NULL,
  checksum TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  schedule TEXT,
  timezone TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'ok',
  validation_errors TEXT,
  config_json TEXT NOT NULL,
  prompt TEXT NOT NULL,
  job_id TEXT,
  last_loaded_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
CREATE INDEX IF NOT EXISTS idx_agents_enabled ON agents(enabled);

-- Scheduled jobs
CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  schedule TEXT NOT NULL,
  operation TEXT NOT NULL,
  payload TEXT,
  next_run_at INTEGER,
  last_run_at INTEGER,
  overlap_policy TEXT DEFAULT 'skip',
  max_retries INTEGER DEFAULT 3,
  timeout_ms INTEGER DEFAULT 30000,
  status TEXT DEFAULT 'active',
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_next_run ON jobs(next_run_at) WHERE enabled = 1;

-- Job execution history
CREATE TABLE IF NOT EXISTS job_runs (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(id),
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  status TEXT NOT NULL,
  result TEXT,
  error TEXT,
  retry_count INTEGER DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id);
`,
  },
  {
    version: 7,
    sql: `
-- Governance event log (separate from session timeline events)
CREATE TABLE IF NOT EXISTS gov_events (
  id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor TEXT NOT NULL,
  session_id TEXT,
  input TEXT,
  output TEXT,
  error TEXT,
  duration_ms INTEGER,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gov_events_operation ON gov_events(operation);
CREATE INDEX IF NOT EXISTS idx_gov_events_created ON gov_events(created_at);
`,
  },
];

export const WORKSPACE_MIGRATIONS: Migration[] = [];

// ---------------------------------------------------------------------------
// Workspace schema — shared across all agent sessions on the same workspace.
// Contains the code index, file tree, and exclude patterns.
// ---------------------------------------------------------------------------

export const WORKSPACE_SCHEMA_SQL = `
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY,
  applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS file_tree (
  path TEXT PRIMARY KEY,
  hash TEXT NOT NULL,
  size INTEGER,
  modified_at INTEGER,
  language TEXT,
  indexed_at INTEGER
);

CREATE TABLE IF NOT EXISTS code_chunks (
  rowid INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  language TEXT,
  chunk_text TEXT NOT NULL,
  embedding BLOB
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON code_chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_language ON code_chunks(language);

CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(
  path,
  chunk_text,
  language,
  content='code_chunks',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS code_chunks_ai AFTER INSERT ON code_chunks BEGIN
  INSERT INTO code_fts(rowid, path, chunk_text, language)
  VALUES (new.rowid, new.path, new.chunk_text, new.language);
END;

CREATE TRIGGER IF NOT EXISTS code_chunks_ad AFTER DELETE ON code_chunks BEGIN
  INSERT INTO code_fts(code_fts, rowid, path, chunk_text, language)
  VALUES ('delete', old.rowid, old.path, old.chunk_text, old.language);
END;

CREATE TRIGGER IF NOT EXISTS code_chunks_au AFTER UPDATE ON code_chunks BEGIN
  INSERT INTO code_fts(code_fts, rowid, path, chunk_text, language)
  VALUES ('delete', old.rowid, old.path, old.chunk_text, old.language);
  INSERT INTO code_fts(rowid, path, chunk_text, language)
  VALUES (new.rowid, new.path, new.chunk_text, new.language);
END;

CREATE TABLE IF NOT EXISTS exclude_patterns (
  pattern TEXT PRIMARY KEY,
  source TEXT NOT NULL DEFAULT 'default'
);
`;

// ---------------------------------------------------------------------------
// Session schema — per-agent conversation, metrics, skills, tools, etc.
// Legacy single-DB mode still includes the workspace tables for backward compat.
// ---------------------------------------------------------------------------

/** @deprecated Use WORKSPACE_SCHEMA_SQL + SESSION_SCHEMA_SQL for new setups. */
export const SCHEMA_SQL = `
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY,
  applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  label TEXT,
  created_at INTEGER NOT NULL,
  last_active_at INTEGER NOT NULL,
  is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS events (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  payload TEXT NOT NULL,
  turn INTEGER,
  session_id TEXT REFERENCES sessions(id),
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_type ON events(type);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
CREATE INDEX IF NOT EXISTS idx_events_turn ON events(turn);
CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id, created_at);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  turn INTEGER NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('system','user','assistant','tool')),
  content TEXT,
  tool_calls TEXT,
  tool_call_id TEXT,
  tokens_in INTEGER,
  tokens_out INTEGER,
  cost_usd REAL,
  session_id TEXT REFERENCES sessions(id),
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_turn ON messages(turn);
CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_role ON messages(role);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, turn);

CREATE TABLE IF NOT EXISTS compaction_markers (
  id TEXT PRIMARY KEY,
  up_to_turn INTEGER NOT NULL,
  summary TEXT NOT NULL,
  token_count INTEGER,
  session_id TEXT REFERENCES sessions(id),
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS code_chunks (
  rowid INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  language TEXT,
  chunk_text TEXT NOT NULL,
  embedding BLOB
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON code_chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_language ON code_chunks(language);

CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(
  path,
  chunk_text,
  language,
  content='code_chunks',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS code_chunks_ai AFTER INSERT ON code_chunks BEGIN
  INSERT INTO code_fts(rowid, path, chunk_text, language)
  VALUES (new.rowid, new.path, new.chunk_text, new.language);
END;

CREATE TRIGGER IF NOT EXISTS code_chunks_ad AFTER DELETE ON code_chunks BEGIN
  INSERT INTO code_fts(code_fts, rowid, path, chunk_text, language)
  VALUES ('delete', old.rowid, old.path, old.chunk_text, old.language);
END;

CREATE TRIGGER IF NOT EXISTS code_chunks_au AFTER UPDATE ON code_chunks BEGIN
  INSERT INTO code_fts(code_fts, rowid, path, chunk_text, language)
  VALUES ('delete', old.rowid, old.path, old.chunk_text, old.language);
  INSERT INTO code_fts(rowid, path, chunk_text, language)
  VALUES (new.rowid, new.path, new.chunk_text, new.language);
END;

CREATE TABLE IF NOT EXISTS file_tree (
  path TEXT PRIMARY KEY,
  hash TEXT NOT NULL,
  size INTEGER,
  modified_at INTEGER,
  language TEXT,
  indexed_at INTEGER
);

CREATE TABLE IF NOT EXISTS tool_cache (
  hash TEXT PRIMARY KEY,
  tool_name TEXT NOT NULL,
  input TEXT NOT NULL,
  output TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS metrics (
  turn INTEGER NOT NULL,
  model TEXT,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL NOT NULL DEFAULT 0,
  tool_calls INTEGER NOT NULL DEFAULT 0,
  elapsed_ms INTEGER,
  session_id TEXT REFERENCES sessions(id),
  created_at INTEGER,
  PRIMARY KEY (session_id, turn)
);

CREATE INDEX IF NOT EXISTS idx_metrics_session ON metrics(session_id, turn);

CREATE TABLE IF NOT EXISTS tools (
  name TEXT PRIMARY KEY,
  definition TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  config TEXT
);

CREATE TABLE IF NOT EXISTS permissions (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  pattern TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS exclude_patterns (
  pattern TEXT PRIMARY KEY,
  source TEXT NOT NULL DEFAULT 'default'
);

CREATE TABLE IF NOT EXISTS skills (
  name TEXT PRIMARY KEY,
  description TEXT NOT NULL,
  instructions TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'builtin',
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS subagent_defs (
  name TEXT PRIMARY KEY,
  skill TEXT NOT NULL REFERENCES skills(name),
  description TEXT NOT NULL,
  allowed_tools TEXT NOT NULL,
  max_iterations INTEGER DEFAULT 10,
  max_cost_usd REAL,
  source TEXT NOT NULL DEFAULT 'builtin',
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS skill_files (
  skill_name TEXT NOT NULL,
  path TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (skill_name, path),
  FOREIGN KEY (skill_name) REFERENCES skills(name) ON DELETE CASCADE
);

-- Governance: policies
CREATE TABLE IF NOT EXISTS policies (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  effect TEXT NOT NULL CHECK(effect IN ('allow','deny','audit','confirm')),
  priority INTEGER DEFAULT 0,
  tool_pattern TEXT,
  path_pattern TEXT,
  cost_condition TEXT,
  args_pattern TEXT,
  actor_pattern TEXT,
  message TEXT,
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Governance: hooks
CREATE TABLE IF NOT EXISTS hooks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  phase TEXT NOT NULL CHECK(phase IN ('pre:validation','pre:injection','post:transform')),
  match_pattern TEXT NOT NULL,
  priority INTEGER DEFAULT 0,
  script TEXT NOT NULL,
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Cloud sync config
CREATE TABLE IF NOT EXISTS sync_config (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Agents (for platform layer)
CREATE TABLE IF NOT EXISTS agents (
  id TEXT PRIMARY KEY,
  dir_path TEXT NOT NULL,
  agent_md_path TEXT NOT NULL,
  checksum TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  schedule TEXT,
  timezone TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'ok',
  validation_errors TEXT,
  config_json TEXT NOT NULL,
  prompt TEXT NOT NULL,
  job_id TEXT,
  last_loaded_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
CREATE INDEX IF NOT EXISTS idx_agents_enabled ON agents(enabled);

-- Scheduled jobs
CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  schedule TEXT NOT NULL,
  operation TEXT NOT NULL,
  payload TEXT,
  next_run_at INTEGER,
  last_run_at INTEGER,
  overlap_policy TEXT DEFAULT 'skip',
  max_retries INTEGER DEFAULT 3,
  timeout_ms INTEGER DEFAULT 30000,
  status TEXT DEFAULT 'active',
  enabled INTEGER DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_next_run ON jobs(next_run_at) WHERE enabled = 1;

-- Job execution history
CREATE TABLE IF NOT EXISTS job_runs (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(id),
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  status TEXT NOT NULL,
  result TEXT,
  error TEXT,
  retry_count INTEGER DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id);

-- Governance event log (separate from session timeline events)
CREATE TABLE IF NOT EXISTS gov_events (
  id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor TEXT NOT NULL,
  session_id TEXT,
  input TEXT,
  output TEXT,
  error TEXT,
  duration_ms INTEGER,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gov_events_operation ON gov_events(operation);
CREATE INDEX IF NOT EXISTS idx_gov_events_created ON gov_events(created_at);
`;

/**
 * Validate that SCHEMA_SQL and MIGRATIONS don't drift apart.
 * Call this in tests to ensure every table created by a migration also exists in SCHEMA_SQL.
 */
export function validateSchemaConsistency(): { ok: boolean; missing: string[] } {
  const tableRe = /CREATE TABLE[^(]*?(\w+)\s*\(/gi;
  const schemaTableNames = new Set<string>();
  let m: RegExpExecArray | null;
  while ((m = tableRe.exec(SCHEMA_SQL)) !== null) {
    schemaTableNames.add(m[1]!.toLowerCase());
  }

  const missing: string[] = [];
  for (const migration of MIGRATIONS) {
    const migRe = /CREATE TABLE[^(]*?(\w+)\s*\(/gi;
    let mm: RegExpExecArray | null;
    while ((mm = migRe.exec(migration.sql)) !== null) {
      const name = mm[1]!.toLowerCase();
      if (!schemaTableNames.has(name)) {
        missing.push(`v${migration.version}: table "${name}" not in SCHEMA_SQL`);
      }
    }
  }

  return { ok: missing.length === 0, missing };
}
