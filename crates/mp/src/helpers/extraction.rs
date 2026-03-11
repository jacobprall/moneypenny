use anyhow::Result;

const EXTRACTION_PROMPT: &str = "\
You are a fact extraction system for an AI agent's long-term memory. \
Analyze the conversation below and extract durable facts worth remembering across sessions.

Rules:
- Extract ONLY facts that are worth remembering in future conversations.
- Each fact must be a self-contained statement — not a sentence fragment.
- Include actionable details (column names, exact values, specific conventions).
- Do NOT extract greetings, pleasantries, or meta-conversation.
- Do NOT extract facts that are already in the existing facts list.
- If nothing is worth extracting, output an empty JSON array: []

Output a JSON array (no markdown fences, no explanation) where each element has:
  {\"content\": \"full fact text\", \"summary\": \"shorter version\", \
\"pointer\": \"2-5 word label\", \"keywords\": \"space separated terms\", \
\"confidence\": 0.0 to 1.0}";

pub async fn extract_facts(
    conn: &rusqlite::Connection,
    provider: &dyn mp_llm::provider::LlmProvider,
    agent_id: &str,
    session_id: &str,
) -> Result<usize> {
    let recent = mp_core::store::log::get_recent_messages(conn, session_id, 6)?;
    if recent.is_empty() {
        return Ok(0);
    }

    let new_messages: Vec<String> = recent
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect();

    let extraction_ctx = mp_core::extraction::assemble_extraction_context(
        conn,
        agent_id,
        session_id,
        &new_messages,
        30,
    )?;

    let messages = vec![
        mp_llm::types::Message::system(EXTRACTION_PROMPT),
        mp_llm::types::Message::user(&extraction_ctx),
    ];

    let config = mp_llm::types::GenerateConfig {
        temperature: Some(0.2),
        max_tokens: Some(2000),
        stop: Vec::new(),
    };

    let response = provider.generate(&messages, &[], &config).await?;
    let text = response.content.unwrap_or_default();

    let json_text = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let candidates = match mp_core::extraction::parse_candidates(json_text) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("extraction parse failed: {e}");
            return Ok(0);
        }
    };

    if candidates.is_empty() {
        return Ok(0);
    }

    let last_msg_id = recent.last().map(|m| m.id.as_str());
    let outcomes =
        mp_core::extraction::run_pipeline(conn, agent_id, session_id, &candidates, last_msg_id)?;

    let extracted = outcomes.iter().filter(|o| o.policy_allowed).count();
    if extracted > 0 {
        tracing::info!(count = extracted, "facts extracted");
    }
    Ok(extracted)
}

const SUMMARIZE_EVERY: usize = 20;
const RECENT_KEEP: usize = 10;

const SUMMARIZE_PROMPT: &str = "\
You are a conversation summarization assistant for an AI agent's long-term memory. \
Given a conversation history (and an optional prior rolling summary), produce a concise \
rolling summary that captures:
- Key facts and topics discussed
- Decisions or conclusions reached
- Important context that would help in future turns

Write in neutral third-person prose. Keep under 200 words. \
If given a prior summary, extend it — do not repeat what is already there.";

pub async fn maybe_summarize_session(
    conn: &rusqlite::Connection,
    provider: &dyn mp_llm::provider::LlmProvider,
    session_id: &str,
) {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if count < SUMMARIZE_EVERY as i64 || count % SUMMARIZE_EVERY as i64 != 0 {
        return;
    }

    let all = match mp_core::store::log::get_messages(conn, session_id) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("summarize: failed to load messages: {e}");
            return;
        }
    };

    let keep = RECENT_KEEP.min(all.len());
    let to_summarize = &all[..all.len().saturating_sub(keep)];
    if to_summarize.is_empty() {
        return;
    }

    let existing: Option<String> = conn
        .query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(None);

    let conv_text = to_summarize
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = match &existing {
        Some(prev) if !prev.trim().is_empty() => {
            format!("Prior summary:\n{prev}\n\nNew conversation to incorporate:\n{conv_text}")
        }
        _ => format!("Conversation:\n{conv_text}"),
    };

    let messages = vec![
        mp_llm::types::Message::system(SUMMARIZE_PROMPT),
        mp_llm::types::Message::user(&user_prompt),
    ];
    let cfg = mp_llm::types::GenerateConfig {
        temperature: Some(0.2),
        max_tokens: Some(600),
        stop: Vec::new(),
    };

    match provider.generate(&messages, &[], &cfg).await {
        Ok(resp) => {
            if let Some(summary) = resp.content {
                if !summary.trim().is_empty() {
                    if let Err(e) = mp_core::store::log::update_summary(conn, session_id, &summary)
                    {
                        tracing::warn!("summarize: failed to save summary: {e}");
                    } else {
                        tracing::debug!(session_id, "rolling session summary updated");
                    }
                }
            }
        }
        Err(e) => tracing::warn!("summarize: LLM call failed: {e}"),
    }
}
