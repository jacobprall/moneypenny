//! Format-specific doc generators — Claude MD, Cursor rules, Cortex skill.

use crate::docs::content;

/// Generate CLAUDE.md content for Claude Code.
pub fn generate_claude_md(agent_conn: Option<&rusqlite::Connection>) -> String {
    let important = "**Important:** These tools are MCP tools served by the Moneypenny sidecar \
process. They must appear in your callable tool list. If they do not, the MCP \
server is not connected — tell the user to run `mp setup claude-code` in the \
project directory.";

    let mut md = format!(
        r#"## Moneypenny

{}

### "mp" prefix

{}

### Tools

{}

{}

### Tool usage

Each domain tool takes an `action` string and an `input` object.

{}

### When to use Moneypenny

{}

### Best practices

{}
"#,
        content::OVERVIEW,
        content::MP_PREFIX,
        content::TOOLS_TABLE,
        important,
        content::TOOL_USAGE_SHORT,
        content::WHEN_TO_USE,
        content::BEST_PRACTICES,
    );

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
}

/// Generate Cursor rules (.mdc) content.
pub fn generate_agent_instructions(agent_conn: Option<&rusqlite::Connection>) -> String {
    let important = "**Important:** These tools are MCP tools served by the Moneypenny sidecar \
process. They must appear in your callable tool list. If they do not, the MCP \
server is not connected — tell the user to run `mp setup cursor` and restart \
Cursor (or reload the window).";

    let mut md = format!(
        r#"---
description: Moneypenny MCP server - persistent facts, knowledge, governance, and activity tracking for AI agents
globs:
alwaysApply: true
---

# Moneypenny

{}

## "mp" prefix

{}

## Tools

{}

{}

## Tool usage

{}

## When to use Moneypenny

{}

## Best practices

{}
"#,
        content::OVERVIEW,
        content::MP_PREFIX,
        content::TOOLS_TABLE,
        important,
        content::TOOL_USAGE_DETAILED,
        content::WHEN_TO_USE,
        content::BEST_PRACTICES,
    );

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
}

/// Generate Cortex skill (SKILL.md) content.
pub fn generate_cortex_skill(agent_conn: Option<&rusqlite::Connection>) -> String {
    let mut md = format!(
        r#"---
name: moneypenny
description: Persistent facts, knowledge, governance, and activity tracking via Moneypenny MCP server
tools:
- mcp__moneypenny__moneypenny_facts
- mcp__moneypenny__moneypenny_knowledge
- mcp__moneypenny__moneypenny_policy
- mcp__moneypenny__moneypenny_activity
- mcp__moneypenny__moneypenny_execute
---

# When to Use

- User says "mp ..." (e.g. "mp remember that we use Redis for caching")
- Remembering things across sessions
- Recalling context or searching memory
- Ingesting documents into knowledge
- Querying the activity/audit trail
- Managing governance policies

# What This Skill Provides

Moneypenny is the intelligence and governance core for AI agents. It provides
structured memory, policy-governed execution, explainable audit, and portable
state — all in a single SQLite file per agent.

# Instructions

Translate natural-language requests into the appropriate MCP tool call and
execute it immediately.

## "mp" Prefix

When the user starts a message with **"mp"**, treat it as a direct instruction
to use Moneypenny. Examples:

- "mp remember that we use Redis for caching" → `moneypenny_facts` action `add`
- "mp search facts about auth" → `moneypenny_facts` action `search`
- "mp ingest this doc" → `moneypenny_knowledge` action `ingest`

## Tool Usage

{}
"#,
        content::TOOL_USAGE_DETAILED,
    );

    md.push_str("\n## Best practices\n\n");
    md.push_str(content::BEST_PRACTICES);

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
}
