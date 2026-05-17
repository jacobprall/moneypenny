-- Track embedding dimensions and model used for each chunk
-- Enables hybrid search even without sqlite-vec extension

ALTER TABLE code_chunks ADD COLUMN embed_model TEXT;
ALTER TABLE code_chunks ADD COLUMN embed_dims INTEGER;
