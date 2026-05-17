# moneypenny — v3 Roadmap

> Everything through v0.2.0 (Phases 1-4) is complete and documented in README.md.

## v3: Advanced intelligence (deferred)

These require either native extensions (sqlite-ai/sqlite-vec), significant R&D, or are dependent on v2 maturity.

### Near-term

- [ ] **sqliteai/sqlite-vector native search** — replace in-JS cosine similarity with sqlite-vec extension for O(1) vector queries
- [ ] **Tree-sitter WASM** — replace regex-based symbol extraction with true AST parsing
- [ ] **Fuzzy convention matching** — deduplicate near-identical conventions from detection runs
- [ ] **Topic extraction** — automatic topic tagging of sessions for better retrieval
- [ ] **Custodian lifecycle** — tier transitions, stale session archival, pointer garbage collection

### Medium-term

- [ ] **Local SLM** — sqlite-ai + GGUF models for offline labeling, compaction, embeddings
- [ ] **Grammar-constrained output** — BNF via sqlite-ai for guaranteed structured extraction
- [ ] **Evolution loops** — self-improving agent prompts based on session outcomes
- [ ] **HTTP API** — REST API for remote access and integrations

### Long-term

- [ ] **Dashboard** — local web UI (Hono JSX) for session viewer, cost charts, skill management
- [ ] **Strategies** — research mode vs standard mode with different tool/model configurations
- [ ] **Child loops / delegation** — sub-agent spawning for parallel task execution
- [ ] **npx moneypenny** — zero-install npm distribution
- [ ] **Homebrew formula** — `brew install moneypenny`

### Deferred
- [ ] **Cloud sync** — sqlite-sync for cross-machine database synchronization
