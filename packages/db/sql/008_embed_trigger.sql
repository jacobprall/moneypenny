-- Auto-flag new code chunks for embedding generation

CREATE TRIGGER IF NOT EXISTS trg_chunk_needs_embedding
AFTER INSERT ON code_chunks
WHEN NEW.embedding IS NULL
BEGIN
    INSERT INTO work_queue (type, payload)
    VALUES ('embed', json_object('chunk_id', NEW.id));
END;
