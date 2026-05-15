# Schema Additions

### Migration strategy

The existing migration system uses `SCHEMA_VERSION` (currently 9) with a
`MIGRATIONS` array in `schema.ts`. Sprint 1 adds migration **version 10**:

```typescript
MIGRATIONS.push({
  version: 10,
  up: (db) => {
    // Sub-agent invocation log
    db.exec(`CREATE TABLE IF NOT EXISTS subagent_runs (...)`);

    // Compaction markers (if not already present from schema.sql)
    db.exec(`CREATE TABLE IF NOT EXISTS compaction_markers (...)`);

    // Agents table additions
    db.exec(`ALTER TABLE agents ADD COLUMN strategy TEXT DEFAULT 'standard'`);
    db.exec(`ALTER TABLE agents ADD COLUMN memory_config TEXT`);
    db.exec(`ALTER TABLE agents ADD COLUMN guardrails TEXT`);
    db.exec(`ALTER TABLE agents ADD COLUMN sub_agents TEXT`);

    // Hooks table migration (declarative)
    db.exec(`ALTER TABLE hooks ADD COLUMN condition TEXT`);
    db.exec(`ALTER TABLE hooks ADD COLUMN action TEXT`);

    // Jobs table additions for generic job types
    db.exec(`ALTER TABLE jobs ADD COLUMN type TEXT DEFAULT 'agents.run'`);
  },
});
```

`SCHEMA_VERSION` bumps to 10. The monolithic `SCHEMA_SQL` is also updated
to include these columns for fresh installs. `validateSchemaConsistency()`
ensures they stay in sync.

### Backward compatibility

All new columns have defaults. Existing databases open and migrate
transparently. No data loss. The `type` column on `jobs` defaults to
`'agents.run'` so existing jobs continue to work.
