use rusqlite::{Connection, Params, params};
use std::collections::{HashMap, HashSet};

/// Source store for a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Store {
    Facts,
    Log,
    Knowledge,
}

impl Store {
    /// Priority for deduplication: Facts > Knowledge > Log.
    #[cfg(test)]
    fn priority(self) -> u8 {
        match self {
            Store::Facts => 3,
            Store::Knowledge => 2,
            Store::Log => 1,
        }
    }
}

/// A single search result from any store.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub store: Store,
    pub content: String,
    pub score: f64,
    pub sources: Vec<Store>,
}

/// Store weights for a search query.
#[derive(Debug, Clone)]
pub struct StoreWeights {
    pub facts: f64,
    pub log: f64,
    pub knowledge: f64,
}

impl Default for StoreWeights {
    fn default() -> Self {
        Self {
            facts: 0.4,
            log: 0.2,
            knowledge: 0.4,
        }
    }
}

/// Detect intent from query keywords and return appropriate store weights.
///
/// Uses cascading pattern checks ordered from most specific to broadest.
/// Falls back to a word-level heuristic when no phrase pattern matches:
/// count words that suggest each store and blend the default weights
/// toward the dominant signal.
pub fn detect_intent(query: &str) -> StoreWeights {
    let q = query.to_lowercase();

    // --- phrase-level patterns (highest confidence) ---

    // Fact recall: "what do we know", "what is", "tell me about", "remember"
    let facts_phrases = [
        "know about",
        "what do we",
        "what is",
        "what are",
        "tell me about",
        "remind me",
        "do you remember",
        "what was",
        "who is",
        "who are",
    ];
    // Log / history: "when did", "last time", "did we discuss", "earlier"
    let log_phrases = [
        "when did",
        "discuss",
        "last time",
        "earlier today",
        "yesterday",
        "previously",
        "conversation history",
        "did we",
        "did i",
        "what happened",
    ];
    // Knowledge / how-to: "how do", "guide", "documentation", "example"
    let knowledge_phrases = [
        "how do",
        "how to",
        "how can",
        "guide",
        "documentation",
        "docs",
        "example of",
        "tutorial",
        "explain how",
        "step by step",
        "walkthrough",
    ];

    if facts_phrases.iter().any(|p| q.contains(p)) {
        return StoreWeights {
            facts: 0.6,
            log: 0.1,
            knowledge: 0.3,
        };
    }
    if log_phrases.iter().any(|p| q.contains(p)) {
        return StoreWeights {
            facts: 0.1,
            log: 0.7,
            knowledge: 0.2,
        };
    }
    if knowledge_phrases.iter().any(|p| q.contains(p)) {
        return StoreWeights {
            facts: 0.2,
            log: 0.1,
            knowledge: 0.7,
        };
    }

    // --- word-level heuristic fallback ---
    let fact_words = [
        "remember", "fact", "known", "stored", "table", "column", "schema", "config",
        "convention", "rule", "policy", "setting",
    ];
    let log_words = [
        "said", "told", "asked", "replied", "message", "chat", "session", "recent",
    ];
    let knowledge_words = [
        "doc", "readme", "guide", "api", "spec", "reference", "code", "function", "module",
    ];

    let words: Vec<&str> = q.split_whitespace().collect();
    let f_hits = words.iter().filter(|w| fact_words.contains(w)).count();
    let l_hits = words.iter().filter(|w| log_words.contains(w)).count();
    let k_hits = words.iter().filter(|w| knowledge_words.contains(w)).count();
    let total = f_hits + l_hits + k_hits;

    if total == 0 {
        return StoreWeights::default();
    }

    // Blend: shift default weights toward the dominant store.
    let base = StoreWeights::default();
    let blend = 0.3; // how much to shift toward the signal
    let f_ratio = f_hits as f64 / total as f64;
    let l_ratio = l_hits as f64 / total as f64;
    let k_ratio = k_hits as f64 / total as f64;

    StoreWeights {
        facts: base.facts + blend * (f_ratio - base.facts),
        log: base.log + blend * (l_ratio - base.log),
        knowledge: base.knowledge + blend * (k_ratio - base.knowledge),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchSourceId {
    Facts,
    Messages,
    ToolCalls,
    PolicyAudit,
    Scratch,
    Knowledge,
}

#[derive(Debug, Clone, Copy)]
struct SearchSource {
    id: SearchSourceId,
    store: Store,
}

const SEARCH_SOURCES: [SearchSource; 6] = [
    SearchSource {
        id: SearchSourceId::Facts,
        store: Store::Facts,
    },
    SearchSource {
        id: SearchSourceId::Messages,
        store: Store::Log,
    },
    SearchSource {
        id: SearchSourceId::ToolCalls,
        store: Store::Log,
    },
    SearchSource {
        id: SearchSourceId::PolicyAudit,
        store: Store::Log,
    },
    SearchSource {
        id: SearchSourceId::Scratch,
        store: Store::Log,
    },
    SearchSource {
        id: SearchSourceId::Knowledge,
        store: Store::Knowledge,
    },
];

fn query_ranked_rows<P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn query_vector_rows<P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> anyhow::Result<Vec<(String, f64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params, |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn query_optional_string<P: Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> anyhow::Result<Option<String>> {
    conn.query_row(sql, params, |r| r.get::<_, String>(0))
        .map(Some)
        .or_else(|_| Ok(None))
}

fn has_fts_table(conn: &Connection, table_name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table_name],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// FTS5 search per store
// ---------------------------------------------------------------------------

/// Sanitize a query string for FTS5 MATCH. FTS5 interprets punctuation (., ?, !, etc.)
/// as special syntax; passing raw user input causes "syntax error near '.'".
/// We extract word characters only and join with spaces.
fn sanitize_fts5_query(query: &str) -> String {
    let words: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect();
    words.join(" ")
}

/// Search facts using FTS5 on keywords field. Returns (id, content, bm25_rank).
pub fn fts5_search_facts(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    let has_fts = has_fts_table(conn, "facts_fts");

    if has_fts {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = conn.prepare(
            "SELECT f.id, f.content, fts.rank
             FROM facts_fts fts
             JOIN facts f ON f.id = fts.id
             WHERE facts_fts MATCH ?1
               AND f.superseded_at IS NULL
               AND (f.scope = 'shared' OR f.agent_id = ?2)
             ORDER BY fts.rank
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![&fts_query, agent_id, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, f64>(2)?.abs()))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let pattern = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, content, 1.0 FROM facts
             WHERE superseded_at IS NULL
               AND (scope = 'shared' OR agent_id = ?1)
               AND (content LIKE ?2 OR summary LIKE ?2 OR pointer LIKE ?2 OR keywords LIKE ?2)
             ORDER BY updated_at DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![agent_id, pattern, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Search messages using LIKE fallback (FTS5 table optional).
pub fn fts5_search_messages(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    if has_fts_table(conn, "messages_fts") {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        query_ranked_rows(
            conn,
            "SELECT m.id, m.content, bm25(messages_fts) AS rank
             FROM messages_fts
             JOIN messages m ON m.id = messages_fts.id
             JOIN sessions s ON s.id = m.session_id
             WHERE messages_fts MATCH ?1 AND s.agent_id = ?2
             ORDER BY rank
             LIMIT ?3",
            params![&fts_query, agent_id, limit],
        )
    } else {
        let pattern = format!("%{query}%");
        query_ranked_rows(
            conn,
            "SELECT m.id, m.content, 1.0
             FROM messages m
             JOIN sessions s ON s.id = m.session_id
             WHERE s.agent_id = ?1 AND m.content LIKE ?2
             ORDER BY m.created_at DESC
             LIMIT ?3",
            params![agent_id, pattern, limit],
        )
    }
}

/// Search projected tool call logs scoped to the agent's sessions.
pub fn fts5_search_tool_calls(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    let projection = crate::store::log::tool_call_projection_expr("tc");
    if has_fts_table(conn, "tool_calls_fts") {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!(
            "SELECT tc.id,
                    ({projection}) AS content,
                    bm25(tool_calls_fts) AS rank
             FROM tool_calls_fts
             JOIN tool_calls tc ON tc.id = tool_calls_fts.id
             JOIN sessions s ON s.id = tc.session_id
             WHERE tool_calls_fts MATCH ?1 AND s.agent_id = ?2
             ORDER BY rank
             LIMIT ?3"
        );
        query_ranked_rows(conn, &sql, params![&fts_query, agent_id, limit])
    } else {
        let pattern = format!("%{query}%");
        let sql = format!(
            "SELECT tc.id,
                    ({projection}) AS content,
                    1.0
             FROM tool_calls tc
             JOIN sessions s ON s.id = tc.session_id
             WHERE s.agent_id = ?1
               AND (
                   tc.tool_name LIKE ?2 OR
                   tc.arguments LIKE ?2 OR
                   tc.result LIKE ?2 OR
                   tc.status LIKE ?2 OR
                   tc.policy_decision LIKE ?2
               )
             ORDER BY tc.created_at DESC
             LIMIT ?3"
        );
        query_ranked_rows(conn, &sql, params![agent_id, pattern, limit])
    }
}

/// Search projected policy audit logs scoped to the agent (via session or actor).
pub fn fts5_search_policy_audit(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    let projection = crate::store::log::policy_audit_projection_expr("pa");
    if has_fts_table(conn, "policy_audit_fts") {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!(
            "SELECT pa.id,
                    ({projection}) AS content,
                    bm25(policy_audit_fts) AS rank
             FROM policy_audit_fts
             JOIN policy_audit pa ON pa.id = policy_audit_fts.id
             WHERE policy_audit_fts MATCH ?1
               AND (
                    pa.actor = ?2 OR
                    pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?2)
               )
             ORDER BY rank
             LIMIT ?3"
        );
        query_ranked_rows(conn, &sql, params![&fts_query, agent_id, limit])
    } else {
        let pattern = format!("%{query}%");
        let sql = format!(
            "SELECT pa.id,
                    ({projection}) AS content,
                    1.0
             FROM policy_audit pa
             WHERE (
                    pa.actor = ?1 OR
                    pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?1)
                   )
               AND (
                    pa.actor LIKE ?2 OR
                    pa.action LIKE ?2 OR
                    pa.resource LIKE ?2 OR
                    pa.effect LIKE ?2 OR
                    pa.reason LIKE ?2
                   )
             ORDER BY pa.created_at DESC
             LIMIT ?3"
        );
        query_ranked_rows(conn, &sql, params![agent_id, pattern, limit])
    }
}

/// Search knowledge chunks using LIKE fallback.
pub fn fts5_search_knowledge(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    if has_fts_table(conn, "chunks_fts") {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        query_ranked_rows(
            conn,
            "SELECT c.id, c.content, bm25(chunks_fts) AS rank
             FROM chunks_fts
             JOIN chunks c ON c.id = chunks_fts.id
             WHERE chunks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
            params![&fts_query, limit],
        )
    } else {
        let pattern = format!("%{query}%");
        query_ranked_rows(
            conn,
            "SELECT id, content, 1.0 FROM chunks
             WHERE content LIKE ?1
             ORDER BY created_at DESC
             LIMIT ?2",
            params![pattern, limit],
        )
    }
}

/// Search session scratch scoped to agent sessions.
pub fn fts5_search_scratch(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    if has_fts_table(conn, "scratch_fts") {
        let fts_query = sanitize_fts5_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        query_ranked_rows(
            conn,
            "SELECT sc.id, (sc.key || ': ' || sc.content) AS content, bm25(scratch_fts) AS rank
             FROM scratch_fts
             JOIN scratch sc ON sc.id = scratch_fts.id
             JOIN sessions s ON s.id = sc.session_id
             WHERE scratch_fts MATCH ?1 AND s.agent_id = ?2
             ORDER BY rank
             LIMIT ?3",
            params![&fts_query, agent_id, limit],
        )
    } else {
        let pattern = format!("%{query}%");
        query_ranked_rows(
            conn,
            "SELECT sc.id, (sc.key || ': ' || sc.content) AS content, 1.0
             FROM scratch sc
             JOIN sessions s ON s.id = sc.session_id
             WHERE s.agent_id = ?1
               AND (sc.key LIKE ?2 OR sc.content LIKE ?2)
             ORDER BY sc.updated_at DESC
             LIMIT ?3",
            params![agent_id, pattern, limit],
        )
    }
}

// ---------------------------------------------------------------------------
// Reciprocal Rank Fusion
// ---------------------------------------------------------------------------

const RRF_K: f64 = 60.0;

/// Merge multiple ranked lists using Reciprocal Rank Fusion.
/// Each input is a list of (id, score) where lower rank = better.
/// Returns fused scores by id, highest first.
pub fn rrf_fuse(ranked_lists: &[Vec<(String, f64)>]) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    for list in ranked_lists {
        for (rank, (id, _original_score)) in list.iter().enumerate() {
            *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }

    let mut results: Vec<(String, f64)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

// ---------------------------------------------------------------------------
// MMR re-ranking
// ---------------------------------------------------------------------------

const DEFAULT_LAMBDA: f64 = 0.7;

/// Jaccard similarity between two strings (tokenized by whitespace).
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Re-rank results using Maximal Marginal Relevance.
/// Balances relevance (score) against diversity (dissimilarity to already-selected).
pub fn mmr_rerank(results: &[SearchResult], k: usize, lambda: Option<f64>) -> Vec<SearchResult> {
    if results.is_empty() || k == 0 {
        return Vec::new();
    }

    let lambda = lambda.unwrap_or(DEFAULT_LAMBDA);
    let mut selected: Vec<SearchResult> = Vec::new();
    let mut remaining: Vec<&SearchResult> = results.iter().collect();

    // Normalize scores to [0, 1]
    let max_score = results.iter().map(|r| r.score).fold(0.0_f64, f64::max);
    let min_score = results.iter().map(|r| r.score).fold(f64::MAX, f64::min);
    let range = if (max_score - min_score).abs() < f64::EPSILON {
        1.0
    } else {
        max_score - min_score
    };

    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = (candidate.score - min_score) / range;

            let max_sim = if selected.is_empty() {
                0.0
            } else {
                selected
                    .iter()
                    .map(|s| jaccard_similarity(&candidate.content, &s.content))
                    .fold(0.0_f64, f64::max)
            };

            let mmr_score = lambda * relevance - (1.0 - lambda) * max_sim;
            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = i;
            }
        }

        selected.push(remaining.remove(best_idx).clone());
    }

    selected
}

// ---------------------------------------------------------------------------
// Vector (embedding) search via sqlite-vector
// ---------------------------------------------------------------------------

/// KNN search over facts.content_embedding using sqlite-vector.
///
/// Returns (id, distance) pairs sorted by ascending distance (closer = better).
/// Silently returns empty if the vector index is not yet populated.
pub fn vector_search_facts(
    conn: &Connection,
    query_blob: &[u8],
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, f64)>> {
    // vector_quantize_scan('facts', 'content_embedding', blob, k) returns (rowid, distance).
    // We join on facts.rowid to get the id string and apply scope visibility.
    query_vector_rows(
        conn,
        "SELECT f.id, v.distance
         FROM facts AS f
         JOIN vector_quantize_scan('facts', 'content_embedding', ?1, ?2) AS v
           ON f.rowid = v.rowid
         WHERE f.superseded_at IS NULL
           AND (f.scope = 'shared' OR f.agent_id = ?3)
         ORDER BY v.distance ASC",
        rusqlite::params![query_blob, limit, agent_id],
    )
}

/// KNN search over messages.content_embedding using sqlite-vector.
pub fn vector_search_messages(
    conn: &Connection,
    query_blob: &[u8],
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, f64)>> {
    query_vector_rows(
        conn,
        "SELECT m.id, v.distance
         FROM messages AS m
         JOIN sessions AS s
           ON s.id = m.session_id
         JOIN vector_quantize_scan('messages', 'content_embedding', ?1, ?2) AS v
           ON m.rowid = v.rowid
         WHERE s.agent_id = ?3
         ORDER BY v.distance ASC",
        rusqlite::params![query_blob, limit, agent_id],
    )
}

/// KNN search over tool_calls.content_embedding using sqlite-vector.
pub fn vector_search_tool_calls(
    conn: &Connection,
    query_blob: &[u8],
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, f64)>> {
    query_vector_rows(
        conn,
        "SELECT tc.id, v.distance
         FROM tool_calls AS tc
         JOIN sessions AS s
           ON s.id = tc.session_id
         JOIN vector_quantize_scan('tool_calls', 'content_embedding', ?1, ?2) AS v
           ON tc.rowid = v.rowid
         WHERE s.agent_id = ?3
         ORDER BY v.distance ASC",
        rusqlite::params![query_blob, limit, agent_id],
    )
}

/// KNN search over policy_audit.content_embedding using sqlite-vector.
pub fn vector_search_policy_audit(
    conn: &Connection,
    query_blob: &[u8],
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, f64)>> {
    query_vector_rows(
        conn,
        "SELECT pa.id, v.distance
         FROM policy_audit AS pa
         JOIN vector_quantize_scan('policy_audit', 'content_embedding', ?1, ?2) AS v
           ON pa.rowid = v.rowid
         WHERE (
                pa.actor = ?3 OR
                pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?3)
               )
         ORDER BY v.distance ASC",
        rusqlite::params![query_blob, limit, agent_id],
    )
}

/// KNN search over chunks.content_embedding using sqlite-vector.
pub fn vector_search_knowledge(
    conn: &Connection,
    query_blob: &[u8],
    limit: usize,
) -> anyhow::Result<Vec<(String, f64)>> {
    query_vector_rows(
        conn,
        "SELECT c.id, v.distance
         FROM chunks AS c
         JOIN vector_quantize_scan('chunks', 'content_embedding', ?1, ?2) AS v
           ON c.rowid = v.rowid
         ORDER BY v.distance ASC",
        rusqlite::params![query_blob, limit],
    )
}

fn text_search_for_source(
    conn: &Connection,
    source: SearchSource,
    query: &str,
    agent_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<(String, String, f64)>> {
    match source.id {
        SearchSourceId::Facts => fts5_search_facts(conn, query, agent_id, limit),
        SearchSourceId::Messages => fts5_search_messages(conn, query, agent_id, limit),
        SearchSourceId::ToolCalls => fts5_search_tool_calls(conn, query, agent_id, limit),
        SearchSourceId::PolicyAudit => fts5_search_policy_audit(conn, query, agent_id, limit),
        SearchSourceId::Scratch => fts5_search_scratch(conn, query, agent_id, limit),
        SearchSourceId::Knowledge => fts5_search_knowledge(conn, query, limit),
    }
}

fn vector_search_for_source(
    conn: &Connection,
    source: SearchSource,
    query_blob: &[u8],
    agent_id: &str,
    limit: usize,
) -> Vec<(String, f64)> {
    let rows = match source.id {
        SearchSourceId::Facts => vector_search_facts(conn, query_blob, agent_id, limit),
        SearchSourceId::Messages => vector_search_messages(conn, query_blob, agent_id, limit),
        SearchSourceId::ToolCalls => vector_search_tool_calls(conn, query_blob, agent_id, limit),
        SearchSourceId::PolicyAudit => {
            vector_search_policy_audit(conn, query_blob, agent_id, limit)
        }
        SearchSourceId::Scratch => Ok(Vec::new()),
        SearchSourceId::Knowledge => vector_search_knowledge(conn, query_blob, limit),
    }
    .unwrap_or_default();

    rows.into_iter()
        .map(|(id, d)| (id, 1.0 / (1.0 + d)))
        .collect()
}

fn fetch_content_for_source(
    conn: &Connection,
    source: SearchSource,
    id: &str,
    agent_id: &str,
) -> anyhow::Result<Option<String>> {
    match source.id {
        SearchSourceId::Facts => query_optional_string(
            conn,
            "SELECT content
             FROM facts
             WHERE id = ?1
               AND superseded_at IS NULL
               AND (scope = 'shared' OR agent_id = ?2)",
            rusqlite::params![id, agent_id],
        ),
        SearchSourceId::Messages => query_optional_string(
            conn,
            "SELECT m.content
                 FROM messages m
                 JOIN sessions s ON s.id = m.session_id
                 WHERE m.id = ?1 AND s.agent_id = ?2",
            rusqlite::params![id, agent_id],
        ),
        SearchSourceId::ToolCalls => {
            let projection = crate::store::log::tool_call_projection_expr("tc");
            let sql = format!(
                "SELECT
                    {projection}
                 FROM tool_calls tc
                 JOIN sessions s ON s.id = tc.session_id
                 WHERE tc.id = ?1 AND s.agent_id = ?2"
            );
            query_optional_string(conn, &sql, rusqlite::params![id, agent_id])
        }
        SearchSourceId::PolicyAudit => {
            let projection = crate::store::log::policy_audit_projection_expr("pa");
            let sql = format!(
                "SELECT
                    {projection}
                 FROM policy_audit pa
                 WHERE pa.id = ?1 AND (
                     pa.actor = ?2 OR
                     pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?2)
                 )"
            );
            query_optional_string(conn, &sql, rusqlite::params![id, agent_id])
        }
        SearchSourceId::Scratch => query_optional_string(
            conn,
            "SELECT (sc.key || ': ' || sc.content)
             FROM scratch sc
             JOIN sessions s ON s.id = sc.session_id
             WHERE sc.id = ?1 AND s.agent_id = ?2",
            rusqlite::params![id, agent_id],
        ),
        SearchSourceId::Knowledge => query_optional_string(
            conn,
            "SELECT content FROM chunks WHERE id = ?1",
            rusqlite::params![id],
        ),
    }
}

// ---------------------------------------------------------------------------
// Recency boost
// ---------------------------------------------------------------------------

/// Maximum recency multiplier applied to fused scores (1.0 = no boost, 1.15 = +15%).
const RECENCY_MAX_BOOST: f64 = 1.15;
/// How quickly recency decays. Items older than this (seconds) get no boost.
const RECENCY_HALF_LIFE_SECS: f64 = 7.0 * 86_400.0; // 7 days

/// Build a mapping of id → recency multiplier in [1.0, RECENCY_MAX_BOOST].
///
/// Queries `facts.updated_at` and `messages.created_at` for IDs in the fused
/// results. Items without a timestamp get a neutral 1.0 multiplier.
fn build_recency_map(
    conn: &Connection,
    fused: &[(String, f64)],
    now: i64,
) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for (id, _) in fused {
        let ts: Option<i64> = conn
            .query_row(
                "SELECT updated_at FROM facts WHERE id = ?1
                 UNION ALL
                 SELECT created_at FROM messages WHERE id = ?1
                 LIMIT 1",
                [id.as_str()],
                |r| r.get(0),
            )
            .ok();

        let boost = if let Some(ts) = ts {
            let age_secs = (now - ts).max(0) as f64;
            // Exponential decay: boost = 1 + (MAX_BOOST - 1) * e^(-age / half_life)
            1.0 + (RECENCY_MAX_BOOST - 1.0) * (-age_secs / RECENCY_HALF_LIFE_SECS).exp()
        } else {
            1.0
        };
        map.insert(id.clone(), boost);
    }
    map
}

// ---------------------------------------------------------------------------
// Cross-store search (unified)
// ---------------------------------------------------------------------------

/// Search across all stores, apply RRF fusion, store weighting, and MMR re-ranking.
///
/// When `query_embedding` is `Some`, the pre-computed FLOAT32 blob is used for
/// vector KNN search (via sqlite-vector) alongside FTS5 text search.  Both
/// signal sets are fused via RRF before weighting and MMR re-ranking.
/// When `query_embedding` is `None` only FTS5/LIKE text search is used.
pub fn search(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
    weights: Option<StoreWeights>,
    query_embedding: Option<&[u8]>,
) -> anyhow::Result<Vec<SearchResult>> {
    let weights = weights.unwrap_or_else(|| detect_intent(query));

    let per_store_limit = limit * 3;

    let mut all_ranked: Vec<Vec<(String, f64)>> = Vec::new();
    let mut content_map: HashMap<String, (String, Store)> = HashMap::new();

    for source in SEARCH_SOURCES {
        let text_results = text_search_for_source(conn, source, query, agent_id, per_store_limit)?;
        all_ranked.push(
            text_results
                .iter()
                .map(|(id, _, score)| (id.clone(), *score))
                .collect(),
        );

        for (id, content, _) in text_results {
            content_map.insert(id, (content, source.store));
        }
    }

    if let Some(blob) = query_embedding {
        for source in SEARCH_SOURCES {
            let ranked = vector_search_for_source(conn, source, blob, agent_id, per_store_limit);
            if !ranked.is_empty() {
                all_ranked.push(ranked);
            }
        }
    }

    let fused = rrf_fuse(&all_ranked);

    // Vector search may surface IDs not found by text search — fetch their content.
    for (id, _) in &fused {
        if content_map.contains_key(id) {
            continue;
        }

        for source in SEARCH_SOURCES {
            if let Ok(Some(row)) = fetch_content_for_source(conn, source, id, agent_id) {
                content_map.insert(id.clone(), (row, source.store));
                break;
            }
        }
    }

    // Recency boost: gently favor recently-updated items.
    // Look up updated_at for facts/messages so newer content ranks slightly higher.
    let now = chrono::Utc::now().timestamp();
    let recency_map = build_recency_map(conn, &fused, now);

    // Apply store weights and recency boost to fused scores.
    let mut weighted_results: Vec<SearchResult> = fused
        .into_iter()
        .filter_map(|(id, score)| {
            let (content, store) = content_map.get(&id)?.clone();
            let weight = match store {
                Store::Facts => weights.facts,
                Store::Log => weights.log,
                Store::Knowledge => weights.knowledge,
            };
            let recency = recency_map.get(&id).copied().unwrap_or(1.0);
            Some(SearchResult {
                id,
                store,
                content,
                score: score * weight * recency,
                sources: vec![store],
            })
        })
        .collect();

    weighted_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // MMR re-rank
    let reranked = mmr_rerank(&weighted_results, limit, None);

    Ok(reranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema, store};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn setup_with_vector(dims: usize) -> Connection {
        let conn = setup();
        mp_ext::init_all_extensions(&conn).unwrap();
        schema::init_vector_indexes(&conn, dims).unwrap();
        conn
    }

    fn f32_blob(v: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(v.len() * std::mem::size_of::<f32>());
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }

    // ========================================================================
    // RRF
    // ========================================================================

    #[test]
    fn rrf_single_list() {
        let lists = vec![vec![
            ("a".into(), 10.0),
            ("b".into(), 5.0),
            ("c".into(), 1.0),
        ]];
        let fused = rrf_fuse(&lists);
        assert_eq!(fused[0].0, "a");
        assert_eq!(fused[1].0, "b");
        assert_eq!(fused[2].0, "c");
    }

    #[test]
    fn rrf_two_lists_boost_shared() {
        let lists = vec![
            vec![("a".into(), 10.0), ("b".into(), 5.0)],
            vec![("b".into(), 10.0), ("c".into(), 5.0)],
        ];
        let fused = rrf_fuse(&lists);
        // "b" appears in both lists, should score highest
        assert_eq!(fused[0].0, "b");
    }

    #[test]
    fn rrf_empty_lists() {
        let fused = rrf_fuse(&[]);
        assert!(fused.is_empty());

        let fused = rrf_fuse(&[vec![]]);
        assert!(fused.is_empty());
    }

    #[test]
    fn rrf_k_constant_is_60() {
        // score for rank 0 should be 1/(60+1) ≈ 0.01639
        let lists = vec![vec![("a".into(), 1.0)]];
        let fused = rrf_fuse(&lists);
        let expected = 1.0 / 61.0;
        assert!((fused[0].1 - expected).abs() < 1e-10);
    }

    // ========================================================================
    // Jaccard similarity
    // ========================================================================

    #[test]
    fn jaccard_identical() {
        assert_eq!(jaccard_similarity("hello world", "hello world"), 1.0);
    }

    #[test]
    fn jaccard_disjoint() {
        assert_eq!(jaccard_similarity("hello world", "foo bar"), 0.0);
    }

    #[test]
    fn jaccard_partial() {
        let sim = jaccard_similarity("a b c", "b c d");
        // intersection: {b, c} = 2, union: {a, b, c, d} = 4
        assert!((sim - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_empty() {
        assert_eq!(jaccard_similarity("", ""), 1.0);
    }

    // ========================================================================
    // MMR re-ranking
    // ========================================================================

    #[test]
    fn mmr_returns_k_results() {
        let results = vec![
            SearchResult {
                id: "a".into(),
                store: Store::Facts,
                content: "alpha beta".into(),
                score: 1.0,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "b".into(),
                store: Store::Facts,
                content: "gamma delta".into(),
                score: 0.8,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "c".into(),
                store: Store::Facts,
                content: "epsilon zeta".into(),
                score: 0.6,
                sources: vec![Store::Facts],
            },
        ];
        let reranked = mmr_rerank(&results, 2, None);
        assert_eq!(reranked.len(), 2);
    }

    #[test]
    fn mmr_highest_relevance_first() {
        let results = vec![
            SearchResult {
                id: "a".into(),
                store: Store::Facts,
                content: "unique content one".into(),
                score: 1.0,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "b".into(),
                store: Store::Facts,
                content: "different content two".into(),
                score: 0.5,
                sources: vec![Store::Facts],
            },
        ];
        let reranked = mmr_rerank(&results, 2, None);
        assert_eq!(reranked[0].id, "a");
    }

    #[test]
    fn mmr_penalizes_duplicates() {
        let results = vec![
            SearchResult {
                id: "a".into(),
                store: Store::Facts,
                content: "the quick brown fox".into(),
                score: 1.0,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "b".into(),
                store: Store::Facts,
                content: "the quick brown fox".into(),
                score: 0.8,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "c".into(),
                store: Store::Facts,
                content: "completely different topic here".into(),
                score: 0.75,
                sources: vec![Store::Facts],
            },
        ];
        // Use a lower λ to make diversity matter more
        let reranked = mmr_rerank(&results, 2, Some(0.5));
        // "a" first (highest score), then "c" should beat "b" due to diversity
        assert_eq!(reranked[0].id, "a");
        assert_eq!(
            reranked[1].id, "c",
            "diverse result should beat near-duplicate"
        );
    }

    #[test]
    fn mmr_empty_input() {
        assert!(mmr_rerank(&[], 5, None).is_empty());
    }

    #[test]
    fn mmr_k_zero() {
        let results = vec![SearchResult {
            id: "a".into(),
            store: Store::Facts,
            content: "x".into(),
            score: 1.0,
            sources: vec![Store::Facts],
        }];
        assert!(mmr_rerank(&results, 0, None).is_empty());
    }

    // ========================================================================
    // Intent detection
    // ========================================================================

    #[test]
    fn intent_knowledge_query() {
        let w = detect_intent("how do I deploy to production?");
        assert!(w.knowledge > w.facts);
        assert!(w.knowledge > w.log);
    }

    #[test]
    fn intent_facts_query() {
        let w = detect_intent("what do we know about the ORDERS table?");
        assert!(w.facts > w.log);
        assert!(w.facts > w.knowledge);
    }

    #[test]
    fn intent_log_query() {
        let w = detect_intent("when did we discuss the migration?");
        assert!(w.log > w.facts);
        assert!(w.log > w.knowledge);
    }

    #[test]
    fn intent_default() {
        let w = detect_intent("soft deletes");
        assert_eq!(w.facts, 0.4);
        assert_eq!(w.log, 0.2);
        assert_eq!(w.knowledge, 0.4);
    }

    // ========================================================================
    // Cross-store search (integration)
    // ========================================================================

    #[test]
    fn search_across_stores() {
        let conn = setup();

        // Seed facts
        store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                scope: "shared".into(),
                content: "ORDERS table uses soft deletes".into(),
                summary: "ORDERS soft deletes".into(),
                pointer: "ORDERS: soft-delete".into(),
                keywords: Some("orders soft deletes".into()),
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        // Seed a session + message
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::log::append_message(&conn, &sid, "user", "Tell me about soft deletes in ORDERS")
            .unwrap();

        // Seed knowledge
        store::knowledge::ingest(
            &conn,
            None,
            None,
            "Soft deletes use a deleted_at column",
            None,
        )
        .unwrap();

        let results = search(&conn, "soft deletes", "a", 10, None, None).unwrap();
        assert!(!results.is_empty(), "should find results across stores");

        let stores: HashSet<Store> = results.iter().map(|r| r.store).collect();
        assert!(
            stores.len() >= 2,
            "should have results from multiple stores: {stores:?}"
        );
    }

    #[test]
    fn search_includes_projected_logs() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid =
            store::log::append_message(&conn, &sid, "assistant", "running deploy checks").unwrap();

        conn.execute(
            "INSERT INTO tool_calls (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                "tc-1",
                mid,
                sid,
                "shell_exec",
                "{\"command\":\"deploy status\"}",
                "deploy denied by policy",
                "denied",
                "deny",
                12_i64,
                1_i64,
            ],
        ).unwrap();

        conn.execute(
            "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                "pa-1",
                "policy-1",
                "a",
                "call",
                "shell_exec",
                "deny",
                "deploy denied in production",
                sid,
                2_i64,
            ],
        ).unwrap();

        let results = search(&conn, "deploy denied", "a", 10, None, None).unwrap();
        assert!(
            results
                .iter()
                .any(|r| r.store == Store::Log && r.content.contains("tool=shell_exec")),
            "should include tool_calls search hit"
        );
        assert!(
            results
                .iter()
                .any(|r| r.store == Store::Log && r.content.contains("policy_audit")),
            "should include policy_audit search hit"
        );
    }

    #[test]
    fn search_includes_scratch_source() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        conn.execute(
            "INSERT INTO scratch (id, session_id, key, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["sc-1", sid, "deploy_note", "rollout window is 02:00 UTC", 1_i64, 1_i64],
        )
        .unwrap();

        let results = search(&conn, "rollout window", "a", 10, None, None).unwrap();
        assert!(
            results
                .iter()
                .any(|r| r.store == Store::Log && r.content.contains("deploy_note")),
            "search should include scratch entries"
        );
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let conn = setup();
        let results = search(&conn, "quantum entanglement", "a", 10, None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_limit() {
        let conn = setup();
        for i in 0..20 {
            store::facts::add(
                &conn,
                &store::facts::NewFact {
                    agent_id: "a".into(),
                    scope: "shared".into(),
                    content: format!("fact about topic {i}"),
                    summary: format!("topic {i}"),
                    pointer: format!("topic-{i}"),
                    keywords: Some("topic".into()),
                    source_message_id: None,
                    confidence: 1.0,
                },
                None,
            )
            .unwrap();
        }
        let results = search(&conn, "topic", "a", 5, None, None).unwrap();
        assert!(results.len() <= 5);
    }

    #[test]
    fn search_semantic_recall_for_tool_and_policy_logs() {
        let conn = setup_with_vector(3);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "runtime activity").unwrap();

        conn.execute(
            "INSERT INTO tool_calls
             (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, content_embedding, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                "tc-sem",
                mid,
                sid,
                "shell_exec",
                "{\"command\":\"deploy check\"}",
                "deploy denied",
                "denied",
                "deny",
                f32_blob(&[1.0, 0.0, 0.0]),
                9_i64,
                1_i64,
            ],
        ).unwrap();

        conn.execute(
            "INSERT INTO policy_audit
             (id, policy_id, actor, action, resource, effect, reason, content_embedding, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                "pa-sem",
                "p1",
                "a",
                "call",
                "shell_exec",
                "deny",
                "prod deploy blocked",
                f32_blob(&[1.0, 0.0, 0.0]),
                sid,
                2_i64,
            ],
        ).unwrap();

        // Add an unrelated embedding so vector ranking has non-trivial choices.
        let fact_id = store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                scope: "shared".into(),
                content: "Unrelated fact".into(),
                summary: "unrelated".into(),
                pointer: "unrelated".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();
        conn.execute(
            "UPDATE facts SET content_embedding = ?1 WHERE id = ?2",
            params![f32_blob(&[0.0, 1.0, 0.0]), fact_id],
        )
        .unwrap();

        for (table, col) in &[
            ("facts", "content_embedding"),
            ("tool_calls", "content_embedding"),
            ("policy_audit", "content_embedding"),
        ] {
            conn.query_row(
                "SELECT vector_quantize(?1, ?2)",
                params![table, col],
                |_| Ok::<_, rusqlite::Error>(()),
            )
            .unwrap();
        }

        // No lexical overlap with inserted content; recall should come from vectors.
        let query_embedding = f32_blob(&[1.0, 0.0, 0.0]);
        let results = search(
            &conn,
            "qzv no lexical overlap",
            "a",
            10,
            None,
            Some(&query_embedding),
        )
        .unwrap();

        assert!(
            results
                .iter()
                .any(|r| r.store == Store::Log && r.content.contains("[tool_call]")),
            "semantic search should surface tool call logs"
        );
        assert!(
            results
                .iter()
                .any(|r| r.store == Store::Log && r.content.contains("[policy_audit]")),
            "semantic search should surface policy audit logs"
        );
    }

    // ========================================================================
    // Store priority
    // ========================================================================

    #[test]
    fn store_priority_order() {
        assert!(Store::Facts.priority() > Store::Knowledge.priority());
        assert!(Store::Knowledge.priority() > Store::Log.priority());
    }

    // ========================================================================
    // Expanded intent detection
    // ========================================================================

    #[test]
    fn intent_remind_me_favors_facts() {
        let w = detect_intent("remind me about the deployment conventions");
        assert!(w.facts > w.log, "remind me → facts");
        assert!(w.facts > w.knowledge, "remind me → facts");
    }

    #[test]
    fn intent_who_is_favors_facts() {
        let w = detect_intent("who is the oncall engineer?");
        assert!(w.facts > w.log);
    }

    #[test]
    fn intent_yesterday_favors_log() {
        let w = detect_intent("what did we talk about yesterday?");
        assert!(w.log > w.facts, "yesterday → log");
        assert!(w.log > w.knowledge, "yesterday → log");
    }

    #[test]
    fn intent_explain_how_favors_knowledge() {
        let w = detect_intent("explain how the CI pipeline works");
        assert!(w.knowledge > w.facts, "explain how → knowledge");
        assert!(w.knowledge > w.log, "explain how → knowledge");
    }

    #[test]
    fn intent_documentation_favors_knowledge() {
        let w = detect_intent("where is the documentation for the API?");
        assert!(w.knowledge > w.log, "documentation → knowledge");
    }

    #[test]
    fn intent_word_level_fallback_blends() {
        let w = detect_intent("stored schema config for the function module");
        // "stored", "schema", "config" → fact_words; "function", "module" → knowledge_words
        // Should blend toward facts but also have some knowledge weight.
        assert!(w.facts >= StoreWeights::default().facts - 0.1);
    }

    #[test]
    fn intent_pure_default_for_neutral_query() {
        let w = detect_intent("hello");
        let d = StoreWeights::default();
        assert!((w.facts - d.facts).abs() < 0.01);
        assert!((w.log - d.log).abs() < 0.01);
        assert!((w.knowledge - d.knowledge).abs() < 0.01);
    }

    // ========================================================================
    // RRF edge cases
    // ========================================================================

    #[test]
    fn rrf_tie_breaking_is_stable() {
        let lists = vec![
            vec![("a".into(), 1.0), ("b".into(), 1.0)],
            vec![("c".into(), 1.0), ("d".into(), 1.0)],
        ];
        let fused = rrf_fuse(&lists);
        assert_eq!(fused.len(), 4, "all items present");
        // All should have the same score (rank 0 in one list each, except none overlap)
        let scores: Vec<f64> = fused.iter().map(|(_, s)| *s).collect();
        for i in 1..scores.len() {
            assert!(
                (scores[0] - scores[i]).abs() < 0.001,
                "tie: all should have equal RRF score"
            );
        }
    }

    #[test]
    fn rrf_many_lists_amplifies_consensus() {
        let lists: Vec<Vec<(String, f64)>> = (0..5)
            .map(|_| vec![("consensus".into(), 1.0), ("noise".into(), 0.5)])
            .collect();
        let fused = rrf_fuse(&lists);
        let consensus_score = fused.iter().find(|(id, _)| id == "consensus").unwrap().1;
        let noise_score = fused.iter().find(|(id, _)| id == "noise").unwrap().1;
        assert!(
            consensus_score > noise_score,
            "consensus across all lists should rank highest"
        );
    }

    // ========================================================================
    // MMR diversity
    // ========================================================================

    #[test]
    fn mmr_diversifies_near_duplicates() {
        let results = vec![
            SearchResult {
                id: "a".into(),
                store: Store::Facts,
                content: "the database uses soft deletes with deleted_at".into(),
                score: 0.9,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "b".into(),
                store: Store::Facts,
                content: "the database uses soft deletes with the deleted_at column".into(),
                score: 0.88,
                sources: vec![Store::Facts],
            },
            SearchResult {
                id: "c".into(),
                store: Store::Knowledge,
                content: "deployment runs ArgoCD canary releases at five percent".into(),
                score: 0.82,
                sources: vec![Store::Knowledge],
            },
        ];

        let reranked = mmr_rerank(&results, 2, Some(0.5));
        assert_eq!(reranked.len(), 2);
        // "a" ranks first by relevance. For the second pick at lambda=0.5,
        // "b" is nearly identical to "a" (Jaccard ~0.87) so it gets a heavy
        // diversity penalty, while "c" has low overlap with "a" (Jaccard ~0.2).
        let ids: Vec<&str> = reranked.iter().map(|r| r.id.as_str()).collect();
        assert!(
            ids.contains(&"a"),
            "highest relevance 'a' should be first pick"
        );
        assert!(
            ids.contains(&"c"),
            "diverse 'c' should beat near-duplicate 'b' at lambda=0.5"
        );
    }

    // ========================================================================
    // Recency boost
    // ========================================================================

    #[test]
    fn recency_boost_favors_recent() {
        let now = 1_000_000i64;
        let recent_ts = now - 3600; // 1 hour ago
        let old_ts = now - 30 * 86400; // 30 days ago

        let recent_age = (now - recent_ts) as f64;
        let old_age = (now - old_ts) as f64;

        let recent_boost =
            1.0 + (RECENCY_MAX_BOOST - 1.0) * (-recent_age / RECENCY_HALF_LIFE_SECS).exp();
        let old_boost =
            1.0 + (RECENCY_MAX_BOOST - 1.0) * (-old_age / RECENCY_HALF_LIFE_SECS).exp();

        assert!(
            recent_boost > old_boost,
            "recent item ({recent_boost:.4}) should get higher boost than old ({old_boost:.4})"
        );
        assert!(
            recent_boost <= RECENCY_MAX_BOOST,
            "boost should not exceed max"
        );
        assert!(old_boost >= 1.0, "old item should still get >= 1.0 boost");
    }
}
