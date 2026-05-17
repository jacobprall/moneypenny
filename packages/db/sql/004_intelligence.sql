-- Intelligence tables: skills, conventions, policies

CREATE TABLE IF NOT EXISTS skills (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    instructions TEXT,
    confidence REAL NOT NULL DEFAULT 0.5,
    source_session_id TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
    name, description, instructions,
    content=skills, content_rowid=rowid
);

CREATE TRIGGER IF NOT EXISTS skills_ai AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, instructions)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.instructions);
END;
CREATE TRIGGER IF NOT EXISTS skills_ad AFTER DELETE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, instructions)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.instructions);
END;
CREATE TRIGGER IF NOT EXISTS skills_au AFTER UPDATE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, instructions)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.instructions);
    INSERT INTO skills_fts(rowid, name, description, instructions)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.instructions);
END;

CREATE TABLE IF NOT EXISTS conventions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    description TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS policies (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    effect TEXT NOT NULL DEFAULT 'warn',
    conditions TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    source_path TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
