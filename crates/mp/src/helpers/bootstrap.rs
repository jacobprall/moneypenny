use mp_core::store::facts::{add, NewFact};

pub fn seed_bootstrap_facts(conn: &rusqlite::Connection, agent_id: &str) {
    let seeds: &[(&str, &str, &str, &str)] = &[
        (
            "Moneypenny: persistent memory, knowledge, policies, tools, extraction",
            "Moneypenny is an autonomous AI agent runtime where the database is the runtime. \
             It provides persistent long-term memory (facts), knowledge retrieval from ingested \
             documents, governance policies, scheduled jobs, and conversation history across sessions.",
            "Moneypenny is an autonomous AI agent platform where the database is the runtime. \
             Core capabilities:\n\
             - Facts: durable knowledge extracted from conversations, stored with confidence scores. \
               Facts are progressively compacted — full content at Level 0, summaries at Level 1, \
               pointers at Level 2. All fact pointers appear in every context window.\n\
             - Knowledge: documents and URLs ingested into a chunk store with FTS5 search.\n\
             - Policies: allow/deny/audit rules governing what the agent can do.\n\
             - Sessions: conversation history with rolling summaries for long conversations.\n\
             - Jobs: cron-scheduled tasks the agent can run autonomously.\n\
             - Scratch: ephemeral per-session working memory for intermediate results.\n\
             Architecture: SQLite-based, local-first, with optional CRDT sync across agents.",
            "moneypenny memory facts knowledge policies sessions jobs architecture",
        ),
        (
            "Tools: memory_search, fact_list, web_search, file_read, scratch_set/get",
            "Available tools: memory_search (semantic + FTS search across facts, messages, knowledge), \
             fact_list (enumerate stored facts), web_search (live internet search), \
             file_read (read local files), scratch_set/scratch_get (session working memory), \
             knowledge_list (ingested documents), job_list (scheduled jobs), \
             policy_list (active policies), audit_query (audit trail).",
            "The agent has access to these tools:\n\
             - memory_search: search across facts, conversation history, and knowledge. Supports \
               both keyword (FTS5) and semantic (vector) search when embeddings are available.\n\
             - fact_list: list all stored facts with pointers and confidence scores.\n\
             - web_search: search the internet for current information.\n\
             - file_read: read files from the local filesystem.\n\
             - scratch_set / scratch_get: save and retrieve ephemeral values within the current session. \
               Use for intermediate results, plans, and working state.\n\
             - knowledge_list: list ingested documents in the knowledge store.\n\
             - job_list: list scheduled jobs and their status.\n\
             - policy_list: list active governance policies.\n\
             - audit_query: search the audit trail for past actions.\n\
             When uncertain about what you know, use memory_search before answering. \
             When asked to remember something, the extraction pipeline handles it automatically — \
             just acknowledge the request.",
            "tools memory_search fact_list web_search file_read scratch knowledge jobs",
        ),
        (
            "Learning: facts extracted automatically from conversations",
            "The agent learns by extracting durable facts from conversations. An extraction pipeline \
             runs after each turn, identifying statements worth remembering. Facts are deduplicated \
             against existing knowledge and stored with confidence scores.",
            "How the agent learns:\n\
             1. After each conversation turn, an extraction pipeline analyzes recent messages.\n\
             2. Candidate facts are identified — statements that are durable, non-obvious, and worth \
                remembering across sessions.\n\
             3. Candidates are deduplicated against existing facts to avoid redundancy.\n\
             4. New facts are stored with confidence scores (0.0-1.0) and linked to their source message.\n\
             5. Over time, fact pointers are progressively compacted to fit more knowledge into the \
                context window. The full content is always available via memory_search.\n\
             6. Facts can be manually inserted via the MPQ language: \
                INSERT INTO facts (\"content\", topic=\"value\", confidence=0.9)\n\
             The agent does not need to explicitly \"save\" facts — the pipeline handles it. \
             When a user says \"remember this\", just acknowledge it.",
            "learning extraction facts pipeline confidence deduplication compaction",
        ),
        (
            "MPQ: query language for memory operations (SEARCH, INSERT, DELETE)",
            "MPQ (Moneypenny Query) is the agent's query language. Key operations: \
             SEARCH facts/knowledge/audit with WHERE filters, SINCE duration, SORT, TAKE. \
             INSERT INTO facts with content and metadata. DELETE FROM facts with conditions.",
            "MPQ (Moneypenny Query) syntax reference:\n\
             - SEARCH <store> [WHERE <filters>] [SINCE <duration>] [| SORT field ASC|DESC] [| TAKE n]\n\
             - INSERT INTO facts (\"content\", key=value ...)\n\
             - UPDATE facts SET key=value WHERE id = \"id\"\n\
             - DELETE FROM facts WHERE <filters>\n\
             - INGEST \"url\"\n\
             - SEARCH audit WHERE <filters> [| TAKE n]\n\n\
             Stores: facts, knowledge, log, audit\n\
             Filters: field = value, field > value, field LIKE \"%pattern%\", AND\n\
             Durations: 7d, 24h, 30m\n\
             Pipeline: chain stages with |\n\
             Multi-statement: separate with ;\n\n\
             Examples:\n\
             SEARCH facts WHERE topic = \"auth\" SINCE 7d | SORT confidence DESC | TAKE 10\n\
             INSERT INTO facts (\"Redis preferred for caching\", topic=\"infra\", confidence=0.9)\n\
             SEARCH facts | COUNT",
            "mpq query language search insert delete facts knowledge audit",
        ),
    ];

    for (pointer, summary, content, keywords) in seeds {
        let fact = NewFact {
            agent_id: agent_id.to_string(),
            scope: "shared".to_string(),
            content: content.to_string(),
            summary: summary.to_string(),
            pointer: pointer.to_string(),
            keywords: Some(keywords.to_string()),
            source_message_id: None,
            confidence: 1.0,
        };
        if let Err(e) = add(conn, &fact, Some("bootstrap")) {
            tracing::warn!(agent = agent_id, "failed to seed bootstrap fact: {e}");
        }
    }
}
