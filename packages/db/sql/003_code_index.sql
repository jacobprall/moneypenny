-- Code indexing: chunks, file tree, FTS

CREATE TABLE IF NOT EXISTS code_chunks (
    id TEXT PRIMARY KEY NOT NULL,
    file_path TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    language TEXT,
    symbol_name TEXT,
    start_line INTEGER,
    end_line INTEGER,
    embedding BLOB,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON code_chunks(file_path);
CREATE INDEX IF NOT EXISTS idx_chunks_symbol ON code_chunks(symbol_name) WHERE symbol_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_chunks_lang ON code_chunks(language);

CREATE VIRTUAL TABLE IF NOT EXISTS code_chunks_fts USING fts5(
    content, symbol_name, file_path,
    content=code_chunks, content_rowid=rowid
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON code_chunks BEGIN
    INSERT INTO code_chunks_fts(rowid, content, symbol_name, file_path)
    VALUES (NEW.rowid, NEW.content, NEW.symbol_name, NEW.file_path);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON code_chunks BEGIN
    INSERT INTO code_chunks_fts(code_chunks_fts, rowid, content, symbol_name, file_path)
    VALUES ('delete', OLD.rowid, OLD.content, OLD.symbol_name, OLD.file_path);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON code_chunks BEGIN
    INSERT INTO code_chunks_fts(code_chunks_fts, rowid, content, symbol_name, file_path)
    VALUES ('delete', OLD.rowid, OLD.content, OLD.symbol_name, OLD.file_path);
    INSERT INTO code_chunks_fts(rowid, content, symbol_name, file_path)
    VALUES (NEW.rowid, NEW.content, NEW.symbol_name, NEW.file_path);
END;

CREATE TABLE IF NOT EXISTS file_tree (
    path TEXT PRIMARY KEY NOT NULL,
    is_dir INTEGER NOT NULL DEFAULT 0,
    size_bytes INTEGER,
    language TEXT,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
