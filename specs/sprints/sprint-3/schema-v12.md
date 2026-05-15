# Schema additions (migration v12)

```typescript
MIGRATIONS.push({
  version: 12,
  up: (db) => {
    // Channel session mapping
    db.exec(`CREATE TABLE IF NOT EXISTS channel_sessions (
      channel_name TEXT NOT NULL,
      external_id TEXT NOT NULL,
      session_id TEXT NOT NULL REFERENCES sessions(id),
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      PRIMARY KEY (channel_name, external_id)
    )`);

    // Prompt refinements
    db.exec(`CREATE TABLE IF NOT EXISTS prompt_refinements (
      id TEXT PRIMARY KEY NOT NULL,
      agent_name TEXT NOT NULL,
      category TEXT NOT NULL,
      content TEXT NOT NULL,
      confidence REAL NOT NULL DEFAULT 0.5,
      status TEXT NOT NULL DEFAULT 'proposed',
      evidence TEXT,
      source_sessions TEXT,
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    )`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_refinements_agent
      ON prompt_refinements(agent_name, status)`);

    // Messages FTS
    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
      content,
      content='messages', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS messages_fts_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS messages_fts_ad AFTER DELETE ON messages BEGIN
      INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    END`);

    // Stable views
    db.exec(`CREATE VIEW IF NOT EXISTS mp_agent_activity AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_tool_usage AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_daily_cost AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_governance_log AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_knowledge AS ...`);
  },
});
```
