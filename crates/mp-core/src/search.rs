use rusqlite::{Connection, params};
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
        Self { facts: 0.4, log: 0.2, knowledge: 0.4 }
    }
}

/// Detect intent from query keywords and return appropriate weights.
pub fn detect_intent(query: &str) -> StoreWeights {
    let q = query.to_lowercase();
    if q.contains("know about") || q.contains("what do") || q.contains("what is") {
        StoreWeights { facts: 0.6, log: 0.1, knowledge: 0.3 }
    } else if q.contains("when did") || q.contains("discuss") || q.contains("last time") {
        StoreWeights { facts: 0.1, log: 0.7, knowledge: 0.2 }
    } else if q.contains("how do") || q.contains("how to") || q.contains("guide") {
        StoreWeights { facts: 0.2, log: 0.1, knowledge: 0.7 }
    } else {
        StoreWeights::default()
    }
}

// ---------------------------------------------------------------------------
// FTS5 search per store
// ---------------------------------------------------------------------------

/// Search facts using FTS5 on keywords field. Returns (id, content, bm25_rank).
pub fn fts5_search_facts(conn: &Connection, query: &str, agent_id: &str, limit: usize) -> anyhow::Result<Vec<(String, String, f64)>> {
    // FTS5 requires a virtual table. If not present, fall back to LIKE.
    let has_fts: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE name = 'facts_fts'",
        [], |r| r.get(0),
    ).unwrap_or(false);

    if has_fts {
        let mut stmt = conn.prepare(
            "SELECT f.id, f.content, fts.rank
             FROM facts_fts fts
             JOIN facts f ON f.id = fts.rowid
             WHERE facts_fts MATCH ?1 AND f.agent_id = ?2 AND f.superseded_at IS NULL
             ORDER BY fts.rank
             LIMIT ?3"
        )?;
        let rows = stmt.query_map(params![query, agent_id, limit], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, f64>(2)?.abs()))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let pattern = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, content, 1.0 FROM facts
             WHERE agent_id = ?1 AND superseded_at IS NULL
               AND (content LIKE ?2 OR summary LIKE ?2 OR pointer LIKE ?2 OR keywords LIKE ?2)
             ORDER BY updated_at DESC
             LIMIT ?3"
        )?;
        let rows = stmt.query_map(params![agent_id, pattern, limit], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Search messages using LIKE fallback (FTS5 table optional).
pub fn fts5_search_messages(conn: &Connection, query: &str, agent_id: &str, limit: usize) -> anyhow::Result<Vec<(String, String, f64)>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT m.id, m.content, 1.0
         FROM messages m
         JOIN sessions s ON s.id = m.session_id
         WHERE s.agent_id = ?1 AND m.content LIKE ?2
         ORDER BY m.created_at DESC
         LIMIT ?3"
    )?;
    let rows = stmt.query_map(params![agent_id, pattern, limit], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Search knowledge chunks using LIKE fallback.
pub fn fts5_search_knowledge(conn: &Connection, query: &str, limit: usize) -> anyhow::Result<Vec<(String, String, f64)>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, content, 1.0 FROM chunks
         WHERE content LIKE ?1
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![pattern, limit], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
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
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
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
    let range = if (max_score - min_score).abs() < f64::EPSILON { 1.0 } else { max_score - min_score };

    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = (candidate.score - min_score) / range;

            let max_sim = if selected.is_empty() {
                0.0
            } else {
                selected.iter()
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
// Cross-store search (unified)
// ---------------------------------------------------------------------------

/// Search across all stores, apply RRF fusion, store weighting, and MMR re-ranking.
pub fn search(
    conn: &Connection,
    query: &str,
    agent_id: &str,
    limit: usize,
    weights: Option<StoreWeights>,
) -> anyhow::Result<Vec<SearchResult>> {
    let weights = weights.unwrap_or_else(|| detect_intent(query));

    let per_store_limit = limit * 3;

    let fact_results = fts5_search_facts(conn, query, agent_id, per_store_limit)?;
    let msg_results = fts5_search_messages(conn, query, agent_id, per_store_limit)?;
    let knowledge_results = fts5_search_knowledge(conn, query, per_store_limit)?;

    // Build ranked lists for RRF
    let fact_ranked: Vec<(String, f64)> = fact_results.iter().map(|(id, _, s)| (id.clone(), *s)).collect();
    let msg_ranked: Vec<(String, f64)> = msg_results.iter().map(|(id, _, s)| (id.clone(), *s)).collect();
    let know_ranked: Vec<(String, f64)> = knowledge_results.iter().map(|(id, _, s)| (id.clone(), *s)).collect();

    let fused = rrf_fuse(&[fact_ranked, msg_ranked, know_ranked]);

    // Build content lookup
    let mut content_map: HashMap<String, (String, Store)> = HashMap::new();
    for (id, content, _) in &fact_results {
        content_map.insert(id.clone(), (content.clone(), Store::Facts));
    }
    for (id, content, _) in &msg_results {
        content_map.insert(id.clone(), (content.clone(), Store::Log));
    }
    for (id, content, _) in &knowledge_results {
        content_map.insert(id.clone(), (content.clone(), Store::Knowledge));
    }

    // Apply store weights to fused scores
    let mut weighted_results: Vec<SearchResult> = fused.into_iter()
        .filter_map(|(id, score)| {
            let (content, store) = content_map.get(&id)?.clone();
            let weight = match store {
                Store::Facts => weights.facts,
                Store::Log => weights.log,
                Store::Knowledge => weights.knowledge,
            };
            Some(SearchResult {
                id,
                store,
                content,
                score: score * weight,
                sources: vec![store],
            })
        })
        .collect();

    weighted_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

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

    // ========================================================================
    // RRF
    // ========================================================================

    #[test]
    fn rrf_single_list() {
        let lists = vec![
            vec![("a".into(), 10.0), ("b".into(), 5.0), ("c".into(), 1.0)],
        ];
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
            SearchResult { id: "a".into(), store: Store::Facts, content: "alpha beta".into(), score: 1.0, sources: vec![Store::Facts] },
            SearchResult { id: "b".into(), store: Store::Facts, content: "gamma delta".into(), score: 0.8, sources: vec![Store::Facts] },
            SearchResult { id: "c".into(), store: Store::Facts, content: "epsilon zeta".into(), score: 0.6, sources: vec![Store::Facts] },
        ];
        let reranked = mmr_rerank(&results, 2, None);
        assert_eq!(reranked.len(), 2);
    }

    #[test]
    fn mmr_highest_relevance_first() {
        let results = vec![
            SearchResult { id: "a".into(), store: Store::Facts, content: "unique content one".into(), score: 1.0, sources: vec![Store::Facts] },
            SearchResult { id: "b".into(), store: Store::Facts, content: "different content two".into(), score: 0.5, sources: vec![Store::Facts] },
        ];
        let reranked = mmr_rerank(&results, 2, None);
        assert_eq!(reranked[0].id, "a");
    }

    #[test]
    fn mmr_penalizes_duplicates() {
        let results = vec![
            SearchResult { id: "a".into(), store: Store::Facts, content: "the quick brown fox".into(), score: 1.0, sources: vec![Store::Facts] },
            SearchResult { id: "b".into(), store: Store::Facts, content: "the quick brown fox".into(), score: 0.8, sources: vec![Store::Facts] },
            SearchResult { id: "c".into(), store: Store::Facts, content: "completely different topic here".into(), score: 0.75, sources: vec![Store::Facts] },
        ];
        // Use a lower λ to make diversity matter more
        let reranked = mmr_rerank(&results, 2, Some(0.5));
        // "a" first (highest score), then "c" should beat "b" due to diversity
        assert_eq!(reranked[0].id, "a");
        assert_eq!(reranked[1].id, "c", "diverse result should beat near-duplicate");
    }

    #[test]
    fn mmr_empty_input() {
        assert!(mmr_rerank(&[], 5, None).is_empty());
    }

    #[test]
    fn mmr_k_zero() {
        let results = vec![
            SearchResult { id: "a".into(), store: Store::Facts, content: "x".into(), score: 1.0, sources: vec![Store::Facts] },
        ];
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
        store::facts::add(&conn, &store::facts::NewFact {
            agent_id: "a".into(),
            content: "ORDERS table uses soft deletes".into(),
            summary: "ORDERS soft deletes".into(),
            pointer: "ORDERS: soft-delete".into(),
            keywords: Some("orders soft deletes".into()),
            source_message_id: None,
            confidence: 1.0,
        }, None).unwrap();

        // Seed a session + message
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::log::append_message(&conn, &sid, "user", "Tell me about soft deletes in ORDERS").unwrap();

        // Seed knowledge
        store::knowledge::ingest(&conn, None, None, "Soft deletes use a deleted_at column", None).unwrap();

        let results = search(&conn, "soft deletes", "a", 10, None).unwrap();
        assert!(!results.is_empty(), "should find results across stores");

        let stores: HashSet<Store> = results.iter().map(|r| r.store).collect();
        assert!(stores.len() >= 2, "should have results from multiple stores: {stores:?}");
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let conn = setup();
        let results = search(&conn, "quantum entanglement", "a", 10, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_limit() {
        let conn = setup();
        for i in 0..20 {
            store::facts::add(&conn, &store::facts::NewFact {
                agent_id: "a".into(),
                content: format!("fact about topic {i}"),
                summary: format!("topic {i}"),
                pointer: format!("topic-{i}"),
                keywords: Some("topic".into()),
                source_message_id: None,
                confidence: 1.0,
            }, None).unwrap();
        }
        let results = search(&conn, "topic", "a", 5, None).unwrap();
        assert!(results.len() <= 5);
    }

    // ========================================================================
    // Store priority
    // ========================================================================

    #[test]
    fn store_priority_order() {
        assert!(Store::Facts.priority() > Store::Knowledge.priority());
        assert!(Store::Knowledge.priority() > Store::Log.priority());
    }
}
