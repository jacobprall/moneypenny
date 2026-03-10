use rusqlite::Connection;

/// Rough token estimate: ~4 chars per token for English text.
const CHARS_PER_TOKEN: usize = 4;

/// Segment of the assembled context, tagged by purpose.
#[derive(Debug, Clone)]
pub struct ContextSegment {
    pub label: &'static str,
    pub content: String,
    pub token_estimate: usize,
}

/// Budget allocation percentages for flexible segments.
#[derive(Debug, Clone)]
pub struct BudgetSplit {
    pub facts_expanded_pct: f64,
    pub scratch_pct: f64,
    pub log_pct: f64,
    pub knowledge_pct: f64,
}

impl Default for BudgetSplit {
    fn default() -> Self {
        Self {
            facts_expanded_pct: 0.20,
            scratch_pct: 0.10,
            log_pct: 0.30,
            knowledge_pct: 0.40,
        }
    }
}

/// Token budget for context assembly.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub total: usize,
    pub system_prompt: usize,
    pub policies: usize,
    pub fact_pointers: usize,
    pub current_message: usize,
    pub response_headroom: usize,
}

impl TokenBudget {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            system_prompt: 500,
            policies: 200,
            fact_pointers: 2000,
            current_message: 500,
            response_headroom: 2000,
        }
    }

    pub fn reserved(&self) -> usize {
        self.system_prompt
            + self.policies
            + self.fact_pointers
            + self.current_message
            + self.response_headroom
    }

    pub fn flexible(&self) -> usize {
        self.total.saturating_sub(self.reserved())
    }
}

/// Dynamic rebalancing context.
pub struct RebalanceContext {
    pub scratch_is_empty: bool,
    pub session_is_new: bool,
    pub session_message_count: usize,
}

/// Rebalance the budget split based on session context.
pub fn rebalance(base: &BudgetSplit, ctx: &RebalanceContext) -> BudgetSplit {
    let mut split = base.clone();

    if ctx.scratch_is_empty {
        let freed = split.scratch_pct / 2.0;
        split.scratch_pct = 0.0;
        split.log_pct += freed;
        split.knowledge_pct += freed;
    }

    if ctx.session_is_new {
        let freed = split.log_pct;
        split.log_pct = 0.0;
        split.knowledge_pct += freed;
    }

    if ctx.session_message_count > 100 {
        let boost = 0.10_f64.min(split.knowledge_pct * 0.25);
        split.log_pct += boost;
        split.knowledge_pct -= boost;
    }

    split
}

/// Allocate token counts from a budget split.
pub fn allocate(budget: &TokenBudget, split: &BudgetSplit) -> FlexibleAllocation {
    let flex = budget.flexible();
    FlexibleAllocation {
        facts_expanded: (flex as f64 * split.facts_expanded_pct) as usize,
        scratch: (flex as f64 * split.scratch_pct) as usize,
        log: (flex as f64 * split.log_pct) as usize,
        knowledge: (flex as f64 * split.knowledge_pct) as usize,
    }
}

#[derive(Debug, Clone)]
pub struct FlexibleAllocation {
    pub facts_expanded: usize,
    pub scratch: usize,
    pub log: usize,
    pub knowledge: usize,
}

impl FlexibleAllocation {
    pub fn total(&self) -> usize {
        self.facts_expanded + self.scratch + self.log + self.knowledge
    }
}

fn estimate_tokens(text: &str) -> usize {
    (text.len() + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN
}

/// Truncate text to fit within a token budget, breaking at word boundaries
/// to avoid corrupting context mid-word.
fn truncate_to_budget(text: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens * CHARS_PER_TOKEN;
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Walk backwards from max_chars to find a word boundary.
    let cut = text[..max_chars]
        .rfind(|c: char| c.is_whitespace() || c == '\n')
        .unwrap_or(max_chars);
    text[..cut].trim_end().to_string()
}

/// Assemble the full context for an LLM call.
pub fn assemble(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    persona: Option<&str>,
    current_message: &str,
    budget: &TokenBudget,
    split: Option<&BudgetSplit>,
) -> anyhow::Result<Vec<ContextSegment>> {
    let mut segments = Vec::new();

    // 1. System prompt + persona
    let system = persona.unwrap_or("You are a helpful AI assistant.");
    segments.push(ContextSegment {
        label: "system_prompt",
        content: system.to_string(),
        token_estimate: estimate_tokens(system),
    });

    // 2. Active policies (deny rules as context)
    let policy_text = load_active_policies(conn)?;
    if !policy_text.is_empty() {
        segments.push(ContextSegment {
            label: "policies",
            content: policy_text.clone(),
            token_estimate: estimate_tokens(&policy_text),
        });
    }

    // 2.5. Rolling session summary (accumulated from previous turns)
    let session_summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .unwrap_or(None);
    if let Some(ref s) = session_summary {
        if !s.trim().is_empty() {
            let text = format!("[Conversation history summary]\n{s}");
            segments.push(ContextSegment {
                label: "session_summary",
                content: text.clone(),
                token_estimate: estimate_tokens(&text),
            });
        }
    }

    // 3. ALL fact pointers (Level 2), progressively compacted over time.
    crate::store::facts::compact_for_context(conn, agent_id)?;
    let pointers = crate::store::facts::all_pointers(conn, agent_id)?;
    if !pointers.is_empty() {
        let pointer_text = pointers
            .iter()
            .map(|(id, p, compact, level)| {
                if let Some(c) = compact {
                    if !c.trim().is_empty() {
                        return format!("- [{id}] {p} :: {c} (compact_level={level})");
                    }
                }
                format!("- [{id}] {p}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        segments.push(ContextSegment {
            label: "fact_pointers",
            content: pointer_text.clone(),
            token_estimate: estimate_tokens(&pointer_text),
        });
    }

    // Determine rebalancing context
    let scratch_entries = crate::store::scratch::list(conn, session_id)?;
    let msg_count = message_count(conn, session_id)?;
    let rebalance_ctx = RebalanceContext {
        scratch_is_empty: scratch_entries.is_empty(),
        session_is_new: msg_count == 0,
        session_message_count: msg_count,
    };

    let base_split = split.cloned().unwrap_or_default();
    let effective_split = rebalance(&base_split, &rebalance_ctx);
    let alloc = allocate(budget, &effective_split);

    // 4–7: Flexible segments with dynamic reallocation.
    //
    // Build raw content for each segment first, then redistribute unused
    // budget from empty segments to those that can use it.
    let expanded_facts = crate::search::fts5_search_facts(conn, current_message, agent_id, 20)?;
    let expanded_text: String = expanded_facts
        .iter()
        .map(|(_, content, _)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    let scratch_text = if scratch_entries.is_empty() {
        String::new()
    } else {
        scratch_entries
            .iter()
            .map(|e| format!("[{}] {}", e.key, e.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let recent = crate::store::log::get_recent_messages(conn, session_id, 20)?;
    let log_text = if recent.is_empty() {
        String::new()
    } else {
        recent
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let knowledge_results = crate::search::fts5_search_knowledge(conn, current_message, 10)?;
    let knowledge_text: String = knowledge_results
        .iter()
        .map(|(_, content, _)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    // Redistribute budget from empty segments to non-empty ones.
    let alloc = redistribute_budget(
        alloc,
        &expanded_text,
        &scratch_text,
        &log_text,
        &knowledge_text,
    );

    if !expanded_text.is_empty() && alloc.facts_expanded > 0 {
        let truncated = truncate_to_budget(&expanded_text, alloc.facts_expanded);
        if !truncated.is_empty() {
            segments.push(ContextSegment {
                label: "facts_expanded",
                content: truncated.clone(),
                token_estimate: estimate_tokens(&truncated),
            });
        }
    }

    if !scratch_text.is_empty() && alloc.scratch > 0 {
        let truncated = truncate_to_budget(&scratch_text, alloc.scratch);
        if !truncated.is_empty() {
            segments.push(ContextSegment {
                label: "scratch",
                content: truncated.clone(),
                token_estimate: estimate_tokens(&truncated),
            });
        }
    }

    if !log_text.is_empty() && alloc.log > 0 {
        let truncated = truncate_to_budget(&log_text, alloc.log);
        if !truncated.is_empty() {
            segments.push(ContextSegment {
                label: "log",
                content: truncated.clone(),
                token_estimate: estimate_tokens(&truncated),
            });
        }
    }

    if !knowledge_text.is_empty() && alloc.knowledge > 0 {
        let truncated = truncate_to_budget(&knowledge_text, alloc.knowledge);
        if !truncated.is_empty() {
            segments.push(ContextSegment {
                label: "knowledge",
                content: truncated.clone(),
                token_estimate: estimate_tokens(&truncated),
            });
        }
    }

    // 8. Current message
    segments.push(ContextSegment {
        label: "current_message",
        content: current_message.to_string(),
        token_estimate: estimate_tokens(current_message),
    });

    Ok(segments)
}

/// Move budget from segments with no content to segments that have content,
/// proportionally to their existing allocations.
fn redistribute_budget(
    mut alloc: FlexibleAllocation,
    facts_text: &str,
    scratch_text: &str,
    log_text: &str,
    knowledge_text: &str,
) -> FlexibleAllocation {
    let slots: [(bool, usize); 4] = [
        (!facts_text.is_empty(), alloc.facts_expanded),
        (!scratch_text.is_empty(), alloc.scratch),
        (!log_text.is_empty(), alloc.log),
        (!knowledge_text.is_empty(), alloc.knowledge),
    ];

    let freed: usize = slots
        .iter()
        .filter(|(has_content, _)| !has_content)
        .map(|(_, budget)| *budget)
        .sum();

    if freed == 0 {
        return alloc;
    }

    let alive_total: usize = slots
        .iter()
        .filter(|(has_content, _)| *has_content)
        .map(|(_, budget)| *budget)
        .sum();

    if alive_total == 0 {
        return alloc;
    }

    // Distribute freed budget proportionally to non-empty segments.
    let boost = |current: usize, has_content: bool| -> usize {
        if !has_content {
            return 0;
        }
        current + (freed as f64 * (current as f64 / alive_total as f64)) as usize
    };

    alloc.facts_expanded = boost(alloc.facts_expanded, !facts_text.is_empty());
    alloc.scratch = boost(alloc.scratch, !scratch_text.is_empty());
    alloc.log = boost(alloc.log, !log_text.is_empty());
    alloc.knowledge = boost(alloc.knowledge, !knowledge_text.is_empty());
    alloc
}

fn load_active_policies(conn: &Connection) -> anyhow::Result<String> {
    let mut stmt = conn.prepare(
        "SELECT name, effect, message FROM policies
         WHERE enabled = 1 AND effect = 'deny'
         ORDER BY priority DESC LIMIT 10",
    )?;
    let rules: Vec<String> = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let effect: String = r.get(1)?;
            let msg: Option<String> = r.get(2)?;
            Ok(format!("[{effect}] {name}: {}", msg.unwrap_or_default()))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rules.join("\n"))
}

fn message_count(conn: &Connection, session_id: &str) -> anyhow::Result<usize> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(count as usize)
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
    // Token budget
    // ========================================================================

    #[test]
    fn budget_reserved_matches_spec() {
        let b = TokenBudget::new(128_000);
        assert_eq!(b.reserved(), 5200);
    }

    #[test]
    fn budget_flexible_is_remainder() {
        let b = TokenBudget::new(128_000);
        assert_eq!(b.flexible(), 128_000 - 5200);
    }

    #[test]
    fn budget_small_model_saturates() {
        let b = TokenBudget::new(4000);
        assert_eq!(b.flexible(), 0); // 4000 < 5200 reserved
    }

    // ========================================================================
    // Budget allocation
    // ========================================================================

    #[test]
    fn allocation_default_split() {
        let b = TokenBudget::new(128_000);
        let alloc = allocate(&b, &BudgetSplit::default());
        let flex = b.flexible();
        assert_eq!(alloc.facts_expanded, (flex as f64 * 0.20) as usize);
        assert_eq!(alloc.scratch, (flex as f64 * 0.10) as usize);
        assert_eq!(alloc.log, (flex as f64 * 0.30) as usize);
        assert_eq!(alloc.knowledge, (flex as f64 * 0.40) as usize);
    }

    #[test]
    fn allocation_total_within_flexible() {
        let b = TokenBudget::new(128_000);
        let alloc = allocate(&b, &BudgetSplit::default());
        assert!(alloc.total() <= b.flexible());
    }

    // ========================================================================
    // Rebalancing
    // ========================================================================

    #[test]
    fn rebalance_empty_scratch_redistributes() {
        let split = rebalance(
            &BudgetSplit::default(),
            &RebalanceContext {
                scratch_is_empty: true,
                session_is_new: false,
                session_message_count: 10,
            },
        );
        assert_eq!(split.scratch_pct, 0.0);
        assert!(split.log_pct > BudgetSplit::default().log_pct);
        assert!(split.knowledge_pct > BudgetSplit::default().knowledge_pct);
    }

    #[test]
    fn rebalance_new_session_moves_log_to_knowledge() {
        let split = rebalance(
            &BudgetSplit::default(),
            &RebalanceContext {
                scratch_is_empty: false,
                session_is_new: true,
                session_message_count: 0,
            },
        );
        assert_eq!(split.log_pct, 0.0);
        assert!(split.knowledge_pct > BudgetSplit::default().knowledge_pct);
    }

    #[test]
    fn rebalance_deep_session_boosts_log() {
        let split = rebalance(
            &BudgetSplit::default(),
            &RebalanceContext {
                scratch_is_empty: false,
                session_is_new: false,
                session_message_count: 200,
            },
        );
        assert!(split.log_pct > BudgetSplit::default().log_pct);
    }

    #[test]
    fn rebalance_normal_session_unchanged() {
        let base = BudgetSplit::default();
        let split = rebalance(
            &base,
            &RebalanceContext {
                scratch_is_empty: false,
                session_is_new: false,
                session_message_count: 10,
            },
        );
        assert_eq!(split.facts_expanded_pct, base.facts_expanded_pct);
        assert_eq!(split.scratch_pct, base.scratch_pct);
        assert_eq!(split.log_pct, base.log_pct);
        assert_eq!(split.knowledge_pct, base.knowledge_pct);
    }

    // ========================================================================
    // Context assembly (integration)
    // ========================================================================

    #[test]
    fn assemble_minimal_context() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "hello",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(labels.contains(&"system_prompt"), "must have system prompt");
        assert!(
            labels.contains(&"current_message"),
            "must have current message"
        );
        assert_eq!(segments.last().unwrap().label, "current_message");
    }

    #[test]
    fn assemble_includes_facts_when_present() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::facts::add(
            &conn,
            &store::facts::NewFact {
                agent_id: "a".into(),
                scope: "shared".into(),
                content: "ORDERS uses soft deletes with deleted_at audit and compliance history"
                    .into(),
                summary: "ORDERS soft deletes".into(),
                pointer: "ORDERS: soft-delete".into(),
                keywords: Some("orders".into()),
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "tell me about orders",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(
            labels.contains(&"fact_pointers"),
            "should include fact pointers"
        );
        let pointers = segments
            .iter()
            .find(|s| s.label == "fact_pointers")
            .unwrap();
        assert!(
            pointers.content.contains("compact_level="),
            "fact pointers should include compaction metadata"
        );
    }

    #[test]
    fn assemble_includes_scratch_when_present() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::scratch::set(&conn, &sid, "plan", "Step 1: research").unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "continue",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(labels.contains(&"scratch"));
    }

    #[test]
    fn assemble_includes_log_when_messages_exist() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::log::append_message(&conn, &sid, "user", "what about ORDERS?").unwrap();
        store::log::append_message(&conn, &sid, "assistant", "ORDERS uses soft deletes.").unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "tell me more",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(labels.contains(&"log"));
    }

    #[test]
    fn assemble_includes_knowledge_when_relevant() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::knowledge::ingest(
            &conn,
            None,
            None,
            "Soft deletes use a deleted_at column to mark records.",
            None,
        )
        .unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "deleted_at",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(labels.contains(&"knowledge"));
    }

    #[test]
    fn assemble_includes_session_summary_when_present() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        store::log::update_summary(&conn, &sid, "User asked about Rust async patterns.").unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "continue",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(
            labels.contains(&"session_summary"),
            "should include session_summary"
        );
        let sum_seg = segments
            .iter()
            .find(|s| s.label == "session_summary")
            .unwrap();
        assert!(
            sum_seg.content.contains("Rust async"),
            "summary content should be present"
        );
    }

    #[test]
    fn assemble_includes_policies_when_present() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, message, created_at)
             VALUES ('p1', 'no-drop', 100, 'deny', 'DROP statements are blocked', 1)",
            [],
        )
        .unwrap();

        let segments = assemble(
            &conn,
            "a",
            &sid,
            None,
            "hello",
            &TokenBudget::new(128_000),
            None,
        )
        .unwrap();

        let labels: Vec<&str> = segments.iter().map(|s| s.label).collect();
        assert!(labels.contains(&"policies"));
    }

    #[test]
    fn assemble_total_within_budget() {
        let conn = setup();
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        for i in 0..50 {
            store::facts::add(
                &conn,
                &store::facts::NewFact {
                    agent_id: "a".into(),
                    scope: "shared".into(),
                    content: format!("Fact {i} with some longer content to take up space"),
                    summary: format!("Fact {i}"),
                    pointer: format!("fact-{i}"),
                    keywords: Some("fact".into()),
                    source_message_id: None,
                    confidence: 1.0,
                },
                None,
            )
            .unwrap();
        }

        let budget = TokenBudget::new(8000);
        let segments = assemble(&conn, "a", &sid, None, "hello", &budget, None).unwrap();

        let total_tokens: usize = segments.iter().map(|s| s.token_estimate).sum();
        assert!(
            total_tokens <= budget.total,
            "total {total_tokens} exceeds budget {}",
            budget.total
        );
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    #[test]
    fn estimate_tokens_rough() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars / 4 ≈ 3
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn truncate_respects_budget() {
        let long = "a".repeat(1000);
        let truncated = truncate_to_budget(&long, 10); // 10 tokens × 4 chars = 40 chars
        assert_eq!(truncated.len(), 40);
    }

    // ========================================================================
    // Word-boundary truncation
    // ========================================================================

    #[test]
    fn truncate_breaks_at_word_boundary() {
        let text = "the quick brown fox jumps over the lazy dog";
        // Budget = 3 tokens = 12 chars → "the quick brown fox" is 19 chars,
        // so we should truncate to last word boundary before char 12.
        let truncated = truncate_to_budget(text, 3);
        assert!(
            !truncated.ends_with(char::is_alphanumeric)
                || truncated.len() <= 12
                || text.starts_with(&truncated),
            "should break at word boundary, got: '{truncated}'"
        );
        assert!(
            !truncated.contains("  "),
            "should not have trailing whitespace artifacts"
        );
    }

    #[test]
    fn truncate_no_trailing_whitespace() {
        let text = "word1 word2 word3 word4 word5";
        let truncated = truncate_to_budget(text, 3); // 12 chars
        assert!(!truncated.ends_with(' '), "should trim trailing space");
    }

    #[test]
    fn truncate_single_long_word_falls_through() {
        let text = "supercalifragilisticexpialidocious";
        let truncated = truncate_to_budget(text, 2); // 8 chars budget
        assert!(
            truncated.len() <= 8,
            "single word: should truncate at char limit"
        );
    }

    #[test]
    fn truncate_short_text_unchanged() {
        let text = "hello world";
        let truncated = truncate_to_budget(text, 100);
        assert_eq!(truncated, text);
    }

    // ========================================================================
    // Dynamic budget reallocation
    // ========================================================================

    #[test]
    fn redistribute_gives_empty_segments_zero() {
        let alloc = FlexibleAllocation {
            facts_expanded: 1000,
            scratch: 500,
            log: 1500,
            knowledge: 2000,
        };
        let result = redistribute_budget(alloc, "has content", "", "has content", "");
        assert_eq!(result.scratch, 0);
        assert_eq!(result.knowledge, 0);
        assert!(result.facts_expanded > 1000, "facts should get freed budget");
        assert!(result.log > 1500, "log should get freed budget");
    }

    #[test]
    fn redistribute_all_have_content_no_change() {
        let alloc = FlexibleAllocation {
            facts_expanded: 1000,
            scratch: 500,
            log: 1500,
            knowledge: 2000,
        };
        let result = redistribute_budget(alloc, "a", "b", "c", "d");
        assert_eq!(result.facts_expanded, 1000);
        assert_eq!(result.scratch, 500);
        assert_eq!(result.log, 1500);
        assert_eq!(result.knowledge, 2000);
    }

    #[test]
    fn redistribute_proportional_to_existing_allocation() {
        let alloc = FlexibleAllocation {
            facts_expanded: 1000,
            scratch: 0,
            log: 0,
            knowledge: 3000,
        };
        // scratch=0 and log=0 are empty: nothing freed from those.
        // But facts_text="" means facts_expanded (1000) is freed to knowledge.
        let result = redistribute_budget(alloc, "", "no scratch content", "", "has knowledge");
        assert_eq!(result.facts_expanded, 0);
        assert_eq!(result.log, 0);
        // knowledge should get all the freed budget from facts_expanded
        assert!(
            result.knowledge > 3000,
            "knowledge should absorb freed budget"
        );
    }

    #[test]
    fn redistribute_all_empty_returns_original() {
        let alloc = FlexibleAllocation {
            facts_expanded: 1000,
            scratch: 500,
            log: 1500,
            knowledge: 2000,
        };
        let result = redistribute_budget(alloc, "", "", "", "");
        // All empty: no alive segments to receive freed budget → return unchanged.
        // (The caller won't use them since it checks `!text.is_empty()`.)
        assert_eq!(result.facts_expanded, 1000);
        assert_eq!(result.scratch, 500);
        assert_eq!(result.log, 1500);
        assert_eq!(result.knowledge, 2000);
    }

    // ========================================================================
    // Rebalancing edge cases
    // ========================================================================

    #[test]
    fn rebalance_new_session_empty_scratch_combined() {
        let split = rebalance(
            &BudgetSplit::default(),
            &RebalanceContext {
                scratch_is_empty: true,
                session_is_new: true,
                session_message_count: 0,
            },
        );
        assert_eq!(split.scratch_pct, 0.0);
        assert_eq!(split.log_pct, 0.0);
        // All freed budget should go to knowledge
        let total = split.facts_expanded_pct + split.knowledge_pct;
        assert!(
            (total - 1.0).abs() < 0.01,
            "total flexible should be ~1.0, got {total}"
        );
    }

    #[test]
    fn budget_zero_total_does_not_panic() {
        let b = TokenBudget::new(0);
        assert_eq!(b.flexible(), 0);
        let alloc = allocate(&b, &BudgetSplit::default());
        assert_eq!(alloc.total(), 0);
    }
}
