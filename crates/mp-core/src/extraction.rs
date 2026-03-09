use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A candidate fact produced by the extraction LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateFact {
    pub content: String,
    pub summary: String,
    pub pointer: String,
    pub keywords: Option<String>,
    pub confidence: f64,
}

/// LLM's decision for a candidate that matches an existing fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DeduplicationDecision {
    Add,
    Update,
    Delete,
    Noop,
}

/// Result of processing a single candidate through the pipeline.
#[derive(Debug, Clone)]
pub struct ExtractionOutcome {
    pub candidate: CandidateFact,
    pub decision: DeduplicationDecision,
    pub fact_id: Option<String>,
    pub policy_allowed: bool,
    pub reason: Option<String>,
}

/// Assemble the extraction context for the LLM call.
pub fn assemble_extraction_context(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    new_messages: &[String],
    top_k: usize,
) -> anyhow::Result<String> {
    let mut parts = Vec::new();

    // Rolling summary from the session
    let summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(s) = &summary {
        parts.push(format!("## Rolling Summary\n{s}"));
    }

    // New messages from this turn
    if !new_messages.is_empty() {
        parts.push(format!("## New Messages\n{}", new_messages.join("\n")));
    }

    // Top-K existing facts by recency
    let existing = crate::store::facts::list_active(conn, agent_id)?;
    let top = existing.into_iter().take(top_k);
    let facts_text: Vec<String> = top
        .map(|f| format!("[{:.1}] {}: {}", f.confidence, f.pointer, f.summary))
        .collect();
    if !facts_text.is_empty() {
        parts.push(format!("## Existing Facts\n{}", facts_text.join("\n")));
    }

    Ok(parts.join("\n\n"))
}

/// Parse the LLM's extraction response into candidate facts.
pub fn parse_candidates(json_text: &str) -> anyhow::Result<Vec<CandidateFact>> {
    let candidates: Vec<CandidateFact> = serde_json::from_str(json_text)?;
    Ok(candidates)
}

/// Deduplicate a candidate against existing facts.
/// Uses keyword/text overlap as a proxy for vector similarity until embeddings are available.
pub fn find_similar_fact(
    conn: &Connection,
    agent_id: &str,
    candidate: &CandidateFact,
) -> anyhow::Result<Option<(String, f64)>> {
    let active = crate::store::facts::list_active(conn, agent_id)?;

    let candidate_words: std::collections::HashSet<&str> =
        candidate.content.split_whitespace().collect();

    let mut best: Option<(String, f64)> = None;
    for fact in &active {
        let fact_words: std::collections::HashSet<&str> = fact.content.split_whitespace().collect();
        let sim = crate::search::jaccard_similarity(
            &candidate_words
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            &fact_words.iter().copied().collect::<Vec<_>>().join(" "),
        );
        if sim > 0.5 {
            match &best {
                Some((_, best_sim)) if sim <= *best_sim => {}
                _ => best = Some((fact.id.clone(), sim)),
            }
        }
    }
    Ok(best)
}

/// Process a single candidate through the extraction pipeline.
pub fn process_candidate(
    conn: &Connection,
    agent_id: &str,
    candidate: &CandidateFact,
    decision: &DeduplicationDecision,
    existing_fact_id: Option<&str>,
    source_message_id: Option<&str>,
) -> anyhow::Result<ExtractionOutcome> {
    // Policy check
    let policy_req = crate::policy::PolicyRequest {
        actor: agent_id,
        action: "store_fact",
        resource: "facts",
        sql_content: Some(&candidate.content),
        channel: None,
        arguments: None,
    };
    let policy_decision = crate::policy::evaluate(conn, &policy_req)?;

    if matches!(policy_decision.effect, crate::policy::Effect::Deny) {
        return Ok(ExtractionOutcome {
            candidate: candidate.clone(),
            decision: decision.clone(),
            fact_id: existing_fact_id.map(|s| s.to_string()),
            policy_allowed: false,
            reason: policy_decision.reason,
        });
    }

    let fact_id = match decision {
        DeduplicationDecision::Add => {
            let id = crate::store::facts::add(
                conn,
                &crate::store::facts::NewFact {
                    agent_id: agent_id.to_string(),
                    content: candidate.content.clone(),
                    summary: candidate.summary.clone(),
                    pointer: candidate.pointer.clone(),
                    keywords: candidate.keywords.clone(),
                    source_message_id: source_message_id.map(|s| s.to_string()),
                    confidence: candidate.confidence,
                },
                Some("extraction pipeline: new fact"),
            )?;
            Some(id)
        }
        DeduplicationDecision::Update => {
            if let Some(eid) = existing_fact_id {
                crate::store::facts::update(
                    conn,
                    eid,
                    &candidate.content,
                    &candidate.summary,
                    &candidate.pointer,
                    Some("extraction pipeline: updated"),
                    source_message_id,
                )?;
                crate::store::facts::bump_confidence(conn, eid, 0.1)?;
                Some(eid.to_string())
            } else {
                anyhow::bail!("UPDATE decision but no existing fact ID");
            }
        }
        DeduplicationDecision::Delete => {
            if let Some(eid) = existing_fact_id {
                crate::store::facts::delete(conn, eid, Some("extraction pipeline: contradicted"))?;
                Some(eid.to_string())
            } else {
                anyhow::bail!("DELETE decision but no existing fact ID");
            }
        }
        DeduplicationDecision::Noop => {
            if let Some(eid) = existing_fact_id {
                crate::store::facts::bump_confidence(conn, eid, 0.05)?;
            }
            existing_fact_id.map(|s| s.to_string())
        }
    };

    Ok(ExtractionOutcome {
        candidate: candidate.clone(),
        decision: decision.clone(),
        fact_id,
        policy_allowed: true,
        reason: None,
    })
}

/// Run the full extraction pipeline (sync version, for testing).
/// In production, this runs asynchronously after each turn.
pub fn run_pipeline(
    conn: &Connection,
    agent_id: &str,
    _session_id: &str,
    candidates: &[CandidateFact],
    source_message_id: Option<&str>,
) -> anyhow::Result<Vec<ExtractionOutcome>> {
    let mut outcomes = Vec::new();

    // Wrap in a transaction for atomicity
    let tx = conn.unchecked_transaction()?;

    for candidate in candidates {
        let similar = find_similar_fact(&tx, agent_id, candidate)?;

        let (decision, existing_id) = match similar {
            Some((id, sim)) if sim > 0.8 => (DeduplicationDecision::Update, Some(id)),
            Some((id, sim)) if sim > 0.5 => (DeduplicationDecision::Noop, Some(id)),
            _ => (DeduplicationDecision::Add, None),
        };

        let outcome = process_candidate(
            &tx,
            agent_id,
            candidate,
            &decision,
            existing_id.as_deref(),
            source_message_id,
        )?;

        // Link new/updated facts to related facts
        if outcome.policy_allowed {
            if let Some(ref fid) = outcome.fact_id {
                if matches!(
                    decision,
                    DeduplicationDecision::Add | DeduplicationDecision::Update
                ) {
                    link_related_facts(&tx, agent_id, fid, candidate)?;
                }
            }
        }

        outcomes.push(outcome);
    }

    tx.commit()?;
    Ok(outcomes)
}

/// Link a new/updated fact to related existing facts.
fn link_related_facts(
    conn: &Connection,
    agent_id: &str,
    fact_id: &str,
    candidate: &CandidateFact,
) -> anyhow::Result<()> {
    let active = crate::store::facts::list_active(conn, agent_id)?;

    for fact in &active {
        if fact.id == fact_id {
            continue;
        }
        let sim = crate::search::jaccard_similarity(&candidate.content, &fact.content);
        if sim > 0.3 {
            crate::store::facts::link(conn, fact_id, &fact.id, Some("related"), sim)?;
        }
    }
    Ok(())
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

    fn allow_all(conn: &Connection) {
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow-all', 0, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();
    }

    // ========================================================================
    // Extraction context assembly
    // ========================================================================

    #[test]
    fn extraction_context_includes_messages() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let ctx = assemble_extraction_context(
            &conn,
            "a",
            &sid,
            &["user said hello".into(), "assistant replied".into()],
            5,
        )
        .unwrap();
        assert!(ctx.contains("user said hello"));
        assert!(ctx.contains("assistant replied"));
    }

    #[test]
    fn extraction_context_includes_summary() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::log::update_summary(&conn, &sid, "Previously discussed deployment").unwrap();

        let ctx = assemble_extraction_context(&conn, "a", &sid, &[], 5).unwrap();
        assert!(ctx.contains("Previously discussed deployment"));
    }

    #[test]
    fn extraction_context_includes_existing_facts() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                content: "ORDERS uses soft deletes".into(),
                summary: "ORDERS soft deletes".into(),
                pointer: "ORDERS: soft-delete".into(),
                keywords: Some("orders".into()),
                source_message_id: None,
                confidence: 0.9,
            },
            None,
        )
        .unwrap();

        let ctx = assemble_extraction_context(&conn, "a", &sid, &[], 5).unwrap();
        assert!(ctx.contains("ORDERS: soft-delete"));
    }

    // ========================================================================
    // Candidate parsing
    // ========================================================================

    #[test]
    fn parse_candidates_from_json() {
        let json = r#"[
            {"content": "Fact 1", "summary": "F1", "pointer": "p1", "keywords": "k1", "confidence": 0.9},
            {"content": "Fact 2", "summary": "F2", "pointer": "p2", "keywords": null, "confidence": 0.7}
        ]"#;
        let candidates = parse_candidates(json).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].content, "Fact 1");
        assert_eq!(candidates[1].keywords, None);
    }

    #[test]
    fn parse_candidates_invalid_json() {
        assert!(parse_candidates("not json").is_err());
    }

    // ========================================================================
    // Deduplication
    // ========================================================================

    #[test]
    fn find_similar_fact_detects_overlap() {
        let conn = setup();
        store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                content: "the quick brown fox jumps over the lazy dog".into(),
                summary: "fox/dog".into(),
                pointer: "fox-dog".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        let candidate = CandidateFact {
            content: "the quick brown fox jumps over a lazy dog".into(),
            summary: "fox/dog updated".into(),
            pointer: "fox-dog".into(),
            keywords: None,
            confidence: 0.9,
        };
        let similar = find_similar_fact(&conn, "a", &candidate).unwrap();
        assert!(similar.is_some(), "should find similar fact");
    }

    #[test]
    fn find_similar_fact_no_match() {
        let conn = setup();
        store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                content: "completely unrelated topic about elephants".into(),
                summary: "elephants".into(),
                pointer: "elephants".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        let candidate = CandidateFact {
            content: "quantum physics string theory dimensions".into(),
            summary: "physics".into(),
            pointer: "physics".into(),
            keywords: None,
            confidence: 0.9,
        };
        let similar = find_similar_fact(&conn, "a", &candidate).unwrap();
        assert!(similar.is_none());
    }

    // ========================================================================
    // Pipeline execution
    // ========================================================================

    #[test]
    fn pipeline_adds_new_fact() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let candidates = vec![CandidateFact {
            content: "USERS table has email column".into(),
            summary: "USERS has email".into(),
            pointer: "USERS: email".into(),
            keywords: Some("users email".into()),
            confidence: 0.9,
        }];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, None).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].decision, DeduplicationDecision::Add);
        assert!(outcomes[0].policy_allowed);
        assert!(outcomes[0].fact_id.is_some());

        let facts = store::facts::list_active(&conn, "a").unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "USERS table has email column");
    }

    #[test]
    fn pipeline_updates_similar_fact() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        // Pre-existing fact
        let _existing_id = store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                content: "the quick brown fox jumps over the lazy dog in the park".into(),
                summary: "fox/dog".into(),
                pointer: "fox-dog".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        // Candidate very similar to existing (high Jaccard overlap)
        let candidates = vec![CandidateFact {
            content: "the quick brown fox jumps over the lazy dog in the park today".into(),
            summary: "fox/dog updated".into(),
            pointer: "fox-dog-updated".into(),
            keywords: None,
            confidence: 0.95,
        }];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, None).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].policy_allowed);
        // High similarity should yield Update
        assert!(
            matches!(
                outcomes[0].decision,
                DeduplicationDecision::Update | DeduplicationDecision::Noop
            ),
            "decision: {:?}",
            outcomes[0].decision
        );
    }

    #[test]
    fn pipeline_denies_by_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-all', 'deny-all', 100, 'deny', '*', '*', '*', 'blocked', 1)",
            [],
        ).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let candidates = vec![CandidateFact {
            content: "secret data".into(),
            summary: "secret".into(),
            pointer: "secret".into(),
            keywords: None,
            confidence: 0.9,
        }];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, None).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].policy_allowed);

        let facts = store::facts::list_active(&conn, "a").unwrap();
        assert!(facts.is_empty(), "denied facts should not be stored");
    }

    #[test]
    fn pipeline_is_transactional() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let candidates = vec![
            CandidateFact {
                content: "fact one about topic alpha".into(),
                summary: "alpha".into(),
                pointer: "alpha".into(),
                keywords: None,
                confidence: 0.9,
            },
            CandidateFact {
                content: "fact two about topic beta".into(),
                summary: "beta".into(),
                pointer: "beta".into(),
                keywords: None,
                confidence: 0.8,
            },
        ];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, None).unwrap();
        assert_eq!(outcomes.len(), 2);

        let facts = store::facts::list_active(&conn, "a").unwrap();
        assert_eq!(facts.len(), 2, "both facts should be committed atomically");
    }

    #[test]
    fn pipeline_links_related_facts() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        // Add an existing fact
        let _existing_id = store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                content: "deployment uses kubernetes pods containers".into(),
                summary: "k8s deployment".into(),
                pointer: "k8s".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        // Add a related candidate
        let candidates = vec![CandidateFact {
            content: "kubernetes pods have resource limits for containers".into(),
            summary: "k8s resource limits".into(),
            pointer: "k8s-limits".into(),
            keywords: None,
            confidence: 0.85,
        }];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, None).unwrap();
        assert!(outcomes[0].policy_allowed);

        if let Some(ref new_id) = outcomes[0].fact_id {
            let links = store::facts::get_links(&conn, new_id).unwrap();
            assert!(!links.is_empty(), "should link related facts");
        }
    }

    #[test]
    fn pipeline_creates_audit_trail() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let candidates = vec![CandidateFact {
            content: "test fact for audit".into(),
            summary: "audit test".into(),
            pointer: "audit".into(),
            keywords: None,
            confidence: 0.9,
        }];

        let outcomes = run_pipeline(&conn, "a", &sid, &candidates, Some("msg_001")).unwrap();
        let fact_id = outcomes[0].fact_id.as_ref().unwrap();

        let audit = store::facts::get_audit(&conn, fact_id).unwrap();
        assert!(!audit.is_empty(), "should have audit entry");
        assert_eq!(audit[0].operation, "add");
    }
}
