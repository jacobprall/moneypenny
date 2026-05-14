import { z } from "zod";
import type { ToolDefinition } from "../types.js";
import { truncate } from "../utils.js";

const MAX_ROWS = 200;

const SCHEMA_DESCRIPTION = `Run a read-only SELECT against the agent SQLite database. Only SELECT statements are permitted.

## sessions
  id           TEXT PK     -- UUIDv7
  label        TEXT NULL   -- short human title (null = unlabelled)
  created_at   INT         -- unix ms
  last_active_at INT       -- unix ms, updated on each turn
  is_active    INT         -- 1 = current session, 0 = inactive

## messages — one row per message in the conversation
  id           TEXT PK
  turn         INT         -- 1-based turn counter, increments each user→assistant round
  role         TEXT        -- 'system' | 'user' | 'assistant' | 'tool'
  content      TEXT NULL   -- message body (null for pure tool-call messages)
  tool_calls   TEXT NULL   -- JSON array of {id, name, input} when role='assistant'
  tool_call_id TEXT NULL   -- set when role='tool', references a tool_calls[].id
  tokens_in    INT NULL    -- prompt tokens for this message
  tokens_out   INT NULL    -- completion tokens for this message
  cost_usd     REAL NULL   -- cost attributed to this message
  session_id   TEXT FK→sessions
  created_at   INT         -- unix ms

## metrics — one row per turn, aggregated cost/perf data
  turn              INT    -- PK with session_id
  model             TEXT   -- model id used for this turn (e.g. 'claude-sonnet-4-6')
  input_tokens      INT
  output_tokens     INT
  cached_input_tokens INT  -- prompt-cache hits
  cost_usd          REAL   -- total cost for the turn
  tool_calls        INT    -- number of tool invocations in the turn
  elapsed_ms        INT    -- wall-clock time for the turn
  session_id        TEXT FK→sessions
  created_at        INT    -- unix ms

## events — timeline of typed events (tool calls, errors, etc.)
  id           TEXT PK
  type         TEXT        -- e.g. 'tool.complete', 'error', 'turn.complete'
  payload      TEXT        -- JSON object with event-specific data
  turn         INT NULL
  session_id   TEXT FK→sessions
  created_at   INT         -- unix ms

## compaction_markers — conversation summaries replacing old turns
  id           TEXT PK
  up_to_turn   INT         -- all turns ≤ this value were compacted
  summary      TEXT        -- LLM-generated summary of compacted turns
  token_count  INT NULL    -- approximate tokens in the summary
  session_id   TEXT FK→sessions
  created_at   INT         -- unix ms

## config — key-value settings
  key          TEXT PK     -- e.g. 'system_instructions', 'agent_name'
  value        TEXT

## skills — agent capabilities loaded from disk or learned
  name         TEXT PK
  description  TEXT
  instructions TEXT        -- full skill body (markdown)
  source       TEXT        -- 'builtin' | 'agent' | 'user' | 'learned'
  created_at   INT         -- unix ms

## policies — governance rules controlling tool access
  id           TEXT PK
  name         TEXT
  effect       TEXT        -- 'allow' | 'deny' | 'audit' | 'confirm'
  priority     INT         -- higher = evaluated first
  tool_pattern TEXT NULL   -- glob matching tool names
  path_pattern TEXT NULL   -- glob matching file paths
  cost_condition TEXT NULL -- e.g. '>0.10' for cost-based gating
  enabled      INT         -- 1 = active
  created_at   INT         -- unix ms
  updated_at   INT         -- unix ms

All timestamps are Unix milliseconds. All FK relationships use session_id.

IMPORTANT: messages and metrics both have per-turn rows. Joining them directly
produces a cartesian product (many messages per turn × one metric row). Either:
  1. Aggregate each table separately in subqueries/CTEs, then join the results, OR
  2. Join ON (session_id, turn) but be aware messages has multiple rows per turn.
For cost/token totals, query metrics alone. For message content, query messages alone.`;

const inputSchema = z.object({
  query: z.string().describe("A read-only SQL SELECT statement to run against the agent database"),
  params: z
    .array(z.union([z.string(), z.number()]))
    .optional()
    .describe("Optional positional bind parameters (? placeholders) for the query"),
});

const ALLOWED_PREFIX = /^\s*(?:--[^\n]*\n\s*)*(SELECT|WITH)\b/i;

function ensureLimit(sql: string): string {
  if (/\bLIMIT\b/i.test(sql)) return sql;
  return `${sql.replace(/;\s*$/, "")} LIMIT ${MAX_ROWS}`;
}

export const queryDbTool: ToolDefinition = {
  name: "query_db",
  description: SCHEMA_DESCRIPTION,
  inputSchema,
  async execute(input, context): Promise<string> {
    try {
      const { query, params } = input as z.infer<typeof inputSchema>;

      if (!ALLOWED_PREFIX.test(query)) {
        return JSON.stringify({ error: "only SELECT statements are permitted" });
      }

      const bounded = ensureLimit(query);
      const db = context.db.db;
      const bindParams = params ?? [];

      db.exec("SAVEPOINT query_db_fence");
      try {
        const stmt = db.prepare(bounded);
        const rows = stmt.all(...bindParams) as Record<string, unknown>[];
        db.exec("ROLLBACK TO query_db_fence");
        db.exec("RELEASE query_db_fence");
        return truncate(JSON.stringify({ rows, row_count: rows.length }));
      } catch (e) {
        try {
          db.exec("ROLLBACK TO query_db_fence");
          db.exec("RELEASE query_db_fence");
        } catch { /* best effort */ }
        const msg = e instanceof Error ? e.message : String(e);
        return JSON.stringify({ error: msg });
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return JSON.stringify({ error: msg });
    }
  },
};
