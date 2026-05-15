# Schema Additions (Migration v11)

```typescript
MIGRATIONS.push({
  version: 11,
  up: (db) => {
    // Session lifecycle columns
    db.exec(`ALTER TABLE sessions ADD COLUMN archived_at INTEGER`);
    db.exec(`ALTER TABLE sessions ADD COLUMN archive_path TEXT`);
    db.exec(`ALTER TABLE sessions ADD COLUMN archive_checksum TEXT`);

    // Session summaries (embedded compaction summaries for search)
    db.exec(`CREATE TABLE IF NOT EXISTS session_summaries (
      id TEXT PRIMARY KEY NOT NULL,
      session_id TEXT NOT NULL REFERENCES sessions(id),
      summary TEXT NOT NULL,
      embedding BLOB,
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      UNIQUE(session_id)
    )`);

    // FTS indexes for unified query
    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
      content, context,
      content='knowledge', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);

    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
      name, description, instructions,
      content='skills', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);

    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS session_summaries_fts USING fts5(
      summary,
      content='session_summaries', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);

    // FTS sync triggers for knowledge
    db.exec(`CREATE TRIGGER IF NOT EXISTS knowledge_fts_ai AFTER INSERT ON knowledge BEGIN
      INSERT INTO knowledge_fts(rowid, content, context) VALUES (new.rowid, new.content, new.context);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS knowledge_fts_ad AFTER DELETE ON knowledge BEGIN
      INSERT INTO knowledge_fts(knowledge_fts, rowid, content, context) VALUES ('delete', old.rowid, old.content, old.context);
    END`);

    // FTS sync triggers for skills
    db.exec(`CREATE TRIGGER IF NOT EXISTS skills_fts_ai AFTER INSERT ON skills BEGIN
      INSERT INTO skills_fts(rowid, name, description, instructions) VALUES (new.rowid, new.name, new.description, new.instructions);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS skills_fts_ad AFTER DELETE ON skills BEGIN
      INSERT INTO skills_fts(skills_fts, rowid, name, description, instructions) VALUES ('delete', old.rowid, old.name, old.description, old.instructions);
    END`);

    // FTS sync triggers for session summaries
    db.exec(`CREATE TRIGGER IF NOT EXISTS session_summaries_fts_ai AFTER INSERT ON session_summaries BEGIN
      INSERT INTO session_summaries_fts(rowid, summary) VALUES (new.rowid, new.summary);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS session_summaries_fts_ad AFTER DELETE ON session_summaries BEGIN
      INSERT INTO session_summaries_fts(session_summaries_fts, rowid, summary) VALUES ('delete', old.rowid, old.summary);
    END`);

    // Computed views
    db.exec(`CREATE VIEW IF NOT EXISTS mp_health AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_suggestions AS ...`);
  },
});
```
