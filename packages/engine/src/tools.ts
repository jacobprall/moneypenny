import { tool } from "ai";
import { z } from "zod";
import type { Database } from "bun:sqlite";
import { hybridSearch } from "./embeddings.js";
import { readFile, writeFile, readdir, stat, mkdir } from "node:fs/promises";
import { join, relative, resolve, dirname } from "node:path";
import { exec } from "node:child_process";

function sanitizeFts(query: string): string {
  return query
    .replace(/[^\w\s]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

const DANGEROUS_SQL = /\b(DROP|DELETE|INSERT|UPDATE|ALTER|CREATE|ATTACH|DETACH|PRAGMA)\b/i;
const MAX_FILE_SIZE = 512 * 1024;
const MAX_COMMAND_TIMEOUT = 30_000;

export function createToolSet(db: Database, filter?: string[]) {
  const all: Record<string, ReturnType<typeof tool>> = {

    // ── File operations ──────────────────────────────────────────

    read_file: tool({
      description:
        "Read the contents of a file. Returns the text content with line numbers.",
      parameters: z.object({
        path: z.string().describe("Relative or absolute file path"),
        start_line: z.number().optional().describe("Start line (1-indexed)"),
        end_line: z.number().optional().describe("End line (inclusive)"),
      }),
      execute: async ({ path: filePath, start_line, end_line }) => {
        try {
          const resolved = resolve(filePath);
          const content = await readFile(resolved, "utf-8");
          const lines = content.split("\n");

          const start = (start_line ?? 1) - 1;
          const end = end_line ?? lines.length;
          const slice = lines.slice(start, end);

          return {
            path: filePath,
            total_lines: lines.length,
            showing: `${start + 1}-${Math.min(end, lines.length)}`,
            content: slice.map((l, i) => `${start + i + 1}|${l}`).join("\n"),
          };
        } catch (err) {
          return { error: `Cannot read ${filePath}: ${err instanceof Error ? err.message : err}` };
        }
      },
    }),

    read_files: tool({
      description:
        "Read multiple files at once. More efficient than individual read_file calls.",
      parameters: z.object({
        paths: z.array(z.string()).describe("Array of file paths to read"),
        max_lines: z.number().default(100).describe("Max lines per file"),
      }),
      execute: async ({ paths, max_lines }) => {
        const results: Array<{ path: string; content?: string; error?: string; lines?: number }> = [];
        for (const p of paths.slice(0, 20)) {
          try {
            const content = await readFile(resolve(p), "utf-8");
            const lines = content.split("\n");
            const truncated = lines.slice(0, max_lines);
            results.push({
              path: p,
              lines: lines.length,
              content: truncated.map((l, i) => `${i + 1}|${l}`).join("\n") +
                (lines.length > max_lines ? `\n... (${lines.length - max_lines} more lines)` : ""),
            });
          } catch (err) {
            results.push({ path: p, error: err instanceof Error ? err.message : String(err) });
          }
        }
        return { count: results.length, files: results };
      },
    }),

    write_file: tool({
      description:
        "Write content to a file. Creates parent directories if needed.",
      parameters: z.object({
        path: z.string().describe("File path to write"),
        content: z.string().describe("Content to write"),
        create_dirs: z.boolean().default(true).describe("Create parent directories"),
      }),
      execute: async ({ path: filePath, content, create_dirs }) => {
        try {
          const resolved = resolve(filePath);
          if (create_dirs) {
            await mkdir(dirname(resolved), { recursive: true });
          }
          await writeFile(resolved, content, "utf-8");
          return {
            path: filePath,
            bytes: Buffer.byteLength(content),
            lines: content.split("\n").length,
          };
        } catch (err) {
          return { error: `Cannot write ${filePath}: ${err instanceof Error ? err.message : err}` };
        }
      },
    }),

    list_directory: tool({
      description:
        "List files and directories at a path with metadata.",
      parameters: z.object({
        path: z.string().default(".").describe("Directory path"),
        recursive: z.boolean().default(false).describe("List recursively"),
        max_depth: z.number().default(3).describe("Max depth for recursive listing"),
      }),
      execute: async ({ path: dirPath, recursive, max_depth }) => {
        const entries: Array<{
          path: string;
          type: "file" | "dir";
          size?: number;
        }> = [];

        async function walk(dir: string, depth: number): Promise<void> {
          if (depth > max_depth) return;
          try {
            const items = await readdir(dir);
            for (const item of items) {
              if (item.startsWith(".") || item === "node_modules") continue;
              const full = join(dir, item);
              try {
                const s = await stat(full);
                const rel = relative(resolve(dirPath), full);
                if (s.isDirectory()) {
                  entries.push({ path: rel + "/", type: "dir" });
                  if (recursive) await walk(full, depth + 1);
                } else {
                  entries.push({ path: rel, type: "file", size: s.size });
                }
              } catch {}
            }
          } catch {}
        }

        await walk(resolve(dirPath), 0);
        return { path: dirPath, count: entries.length, entries: entries.slice(0, 200) };
      },
    }),

    run_command: tool({
      description:
        "Execute a shell command and return stdout/stderr. Use for builds, tests, git, etc.",
      parameters: z.object({
        command: z.string().describe("Shell command to execute"),
        cwd: z.string().optional().describe("Working directory"),
        timeout_ms: z.number().default(30000).describe("Timeout in milliseconds"),
      }),
      execute: async ({ command, cwd, timeout_ms }) => {
        return new Promise((resolve) => {
          const timeout = Math.min(timeout_ms, MAX_COMMAND_TIMEOUT);
          exec(
            command,
            {
              cwd: cwd ?? process.cwd(),
              timeout,
              maxBuffer: 1024 * 1024,
            },
            (err, stdout, stderr) => {
              resolve({
                command,
                exit_code: err?.code ?? 0,
                stdout: stdout.slice(0, 10000),
                stderr: stderr.slice(0, 5000),
              });
            },
          );
        });
      },
    }),

    // ── Search ───────────────────────────────────────────────────

    search_code: tool({
      description:
        "Search indexed code using hybrid search (keyword + semantic when embeddings available).",
      parameters: z.object({
        query: z.string().describe("Natural language or keyword search"),
        limit: z.number().default(10).describe("Max results"),
      }),
      execute: async ({ query, limit }) => {
        try {
          const results = await hybridSearch(db, query, limit);
          return {
            query,
            count: results.length,
            results: results.map((r) => ({
              path: r.file_path,
              symbol: r.symbol_name,
              line: r.start_line,
              fts_score: r.fts_rank,
              semantic_score: r.semantic_score,
              combined: r.combined_score,
              content: r.content.slice(0, 2000),
            })),
          };
        } catch {
          const ftsQuery = sanitizeFts(query);
          if (!ftsQuery) return { query, count: 0, results: [] };
          const results = db
            .query<
              {
                file_path: string;
                symbol_name: string | null;
                content: string;
                start_line: number | null;
              },
              [string, number]
            >(
              `SELECT c.file_path, c.symbol_name, c.content, c.start_line
               FROM code_chunks_fts fts
               JOIN code_chunks c ON c.rowid = fts.rowid
               WHERE code_chunks_fts MATCH ?
               ORDER BY rank LIMIT ?`,
            )
            .all(ftsQuery, limit);

          return {
            query,
            count: results.length,
            results: results.map((r) => ({
              path: r.file_path,
              symbol: r.symbol_name,
              line: r.start_line,
              content: r.content.slice(0, 2000),
            })),
          };
        }
      },
    }),

    search_messages: tool({
      description: "Search across all conversation history by keyword.",
      parameters: z.object({
        query: z.string().describe("Search query"),
        limit: z.number().default(10).describe("Max results"),
      }),
      execute: async ({ query, limit }) => {
        const ftsQuery = sanitizeFts(query);
        if (!ftsQuery) return { query, count: 0, results: [] };
        const results = db
          .query<
            { content: string; session_id: string; role: string; created_at: number },
            [string, number]
          >(
            `SELECT m.content, m.session_id, m.role, m.created_at
             FROM messages_fts fts
             JOIN messages m ON m.rowid = fts.rowid
             WHERE messages_fts MATCH ?
             ORDER BY rank LIMIT ?`,
          )
          .all(ftsQuery, limit);

        return {
          query,
          count: results.length,
          results: results.map((r) => ({
            role: r.role,
            session_id: r.session_id,
            date: new Date(r.created_at * 1000).toISOString().split("T")[0],
            snippet: r.content.slice(0, 500),
          })),
        };
      },
    }),

    // ── Memory ───────────────────────────────────────────────────

    save_memory: tool({
      description:
        "Persist a piece of knowledge or observation for future recall. Use for facts, decisions, user preferences, or anything worth remembering across sessions.",
      parameters: z.object({
        content: z.string().describe("The knowledge or observation to save"),
        tags: z.string().optional().describe("Comma-separated tags for categorization"),
      }),
      execute: async ({ content, tags }) => {
        const id = crypto.randomUUID();
        db.query(
          `INSERT INTO events (type, detail, created_at)
           VALUES ('memory', json_object('content', ?, 'tags', ?), unixepoch())`,
        ).run(content, tags ?? null);
        return { saved: true, id, content: content.slice(0, 100) };
      },
    }),

    recall_memory: tool({
      description:
        "Search saved memories and past observations. Use before making assumptions.",
      parameters: z.object({
        query: z.string().describe("What to search for in memory"),
        limit: z.number().default(10).describe("Max results"),
      }),
      execute: async ({ query, limit }) => {
        const results = db
          .query<{ detail: string; created_at: number }, [number]>(
            `SELECT detail, created_at FROM events
             WHERE type = 'memory' AND detail LIKE '%' || ? || '%'
             ORDER BY created_at DESC LIMIT ?`,
          )
          .all(limit);

        return {
          query,
          count: results.length,
          memories: results.map((r) => {
            try {
              const parsed = JSON.parse(r.detail);
              return {
                content: parsed.content,
                tags: parsed.tags,
                date: new Date(r.created_at * 1000).toISOString().split("T")[0],
              };
            } catch {
              return { content: r.detail, date: new Date(r.created_at * 1000).toISOString().split("T")[0] };
            }
          }),
        };
      },
    }),

    // ── Session management ───────────────────────────────────────

    expand_previous_session: tool({
      description:
        "Retrieve the full summary for a previous session by its key.",
      parameters: z.object({
        key: z.string().describe("The kebab-case session key"),
      }),
      execute: async ({ key }) => {
        const pointer = db
          .query<
            { key: string; phrase: string; summary: string | null; created_at: number; pinned: number },
            [string]
          >(
            `SELECT key, phrase, summary, created_at, pinned
             FROM session_pointers WHERE key = ? AND archived = 0`,
          )
          .get(key);

        if (!pointer) return { error: `No session found with key: ${key}` };

        return {
          key: pointer.key,
          phrase: pointer.phrase,
          date: new Date(pointer.created_at * 1000).toISOString().split("T")[0],
          pinned: Boolean(pointer.pinned),
          summary: pointer.summary ?? "No detailed summary available yet.",
        };
      },
    }),

    get_full_session: tool({
      description: "Load the complete message transcript of a previous session.",
      parameters: z.object({ key: z.string().describe("The kebab-case session key") }),
      execute: async ({ key }) => {
        const pointer = db
          .query<{ session_id: string }, [string]>(
            "SELECT session_id FROM session_pointers WHERE key = ? AND archived = 0",
          )
          .get(key);

        if (!pointer) return { error: `No session found with key: ${key}` };

        const messages = db
          .query<{ role: string; content: string; created_at: number }, [string]>(
            `SELECT role, content, created_at FROM messages
             WHERE session_id = ? AND content IS NOT NULL ORDER BY turn ASC`,
          )
          .all(pointer.session_id);

        return {
          key, session_id: pointer.session_id, turn_count: messages.length,
          messages: messages.map((m) => ({
            role: m.role, content: m.content.slice(0, 4000),
            date: new Date(m.created_at * 1000).toISOString(),
          })),
        };
      },
    }),

    pin_session: tool({
      description: "Pin a session pointer so it always appears in context.",
      parameters: z.object({ key: z.string().describe("Session pointer key to pin") }),
      execute: async ({ key }) => {
        const result = db
          .query<{ id: string }, [string]>(
            "SELECT id FROM session_pointers WHERE key = ? AND archived = 0",
          )
          .get(key);
        if (!result) return { error: `No session found with key: ${key}` };
        db.query("UPDATE session_pointers SET pinned = 1 WHERE key = ?").run(key);
        return { success: true, key, pinned: true };
      },
    }),

    unpin_session: tool({
      description: "Unpin a session pointer.",
      parameters: z.object({ key: z.string().describe("Session pointer key to unpin") }),
      execute: async ({ key }) => {
        db.query("UPDATE session_pointers SET pinned = 0 WHERE key = ?").run(key);
        return { success: true, key, pinned: false };
      },
    }),

    list_sessions: tool({
      description: "Browse recent sessions with optional filtering.",
      parameters: z.object({
        limit: z.number().default(10).describe("Max sessions to return"),
        active_only: z.boolean().default(false).describe("Only active sessions"),
      }),
      execute: async ({ limit, active_only }) => {
        const where = active_only ? "WHERE is_active = 1" : "";
        const sessions = db
          .query<
            { id: string; label: string | null; agent_name: string | null; is_active: number; created_at: number },
            [number]
          >(`SELECT id, label, agent_name, is_active, created_at FROM sessions ${where} ORDER BY created_at DESC LIMIT ?`)
          .all(limit);

        return {
          count: sessions.length,
          sessions: sessions.map((s) => ({
            id: s.id, label: s.label, agent: s.agent_name,
            active: Boolean(s.is_active),
            date: new Date(s.created_at * 1000).toISOString().split("T")[0],
          })),
        };
      },
    }),

    // ── Knowledge management ─────────────────────────────────────

    learn_skill: tool({
      description: "Teach the agent a new skill or technique to remember.",
      parameters: z.object({
        name: z.string().describe("Short name (3-6 words)"),
        description: z.string().describe("One-sentence description"),
        instructions: z.string().optional().describe("Detailed how-to"),
      }),
      execute: async ({ name, description, instructions }) => {
        const existing = db.query<{ id: string }, [string]>("SELECT id FROM skills WHERE name = ?").get(name);
        if (existing) {
          db.query("UPDATE skills SET description = ?, instructions = ?, confidence = MIN(1.0, confidence + 0.2), updated_at = unixepoch() WHERE id = ?")
            .run(description, instructions ?? null, existing.id);
          return { success: true, action: "updated", name };
        }
        db.query(`INSERT INTO skills (id, name, description, instructions, confidence, created_at, updated_at) VALUES (?, ?, ?, ?, 0.8, unixepoch(), unixepoch())`)
          .run(crypto.randomUUID(), name, description, instructions ?? null);
        return { success: true, action: "created", name };
      },
    }),

    add_convention: tool({
      description: "Add a project convention the agent should follow.",
      parameters: z.object({
        name: z.string().describe("Convention name"),
        category: z.string().default("general").describe("Category"),
        description: z.string().describe("Convention description"),
      }),
      execute: async ({ name, category, description }) => {
        const existing = db.query<{ id: string }, [string]>("SELECT id FROM conventions WHERE name = ?").get(name);
        if (existing) {
          db.query("UPDATE conventions SET description = ?, category = ?, confidence = 1.0 WHERE id = ?")
            .run(description, category, existing.id);
          return { success: true, action: "updated", name };
        }
        db.query(`INSERT INTO conventions (id, name, category, description, confidence, created_at) VALUES (?, ?, ?, ?, 1.0, unixepoch())`)
          .run(`user:${crypto.randomUUID().slice(0, 8)}`, name, category, description);
        return { success: true, action: "created", name };
      },
    }),

    // ── Introspection ────────────────────────────────────────────

    query_db: tool({
      description:
        "Execute a read-only SQL query against the intelligence database. Use for introspection — checking session counts, cost totals, skill confidence, convention lists, work queue status, etc.",
      parameters: z.object({
        sql: z.string().describe("SELECT query (read-only, no mutations)"),
      }),
      execute: async ({ sql }) => {
        if (DANGEROUS_SQL.test(sql)) {
          return { error: "Only SELECT queries are allowed." };
        }
        try {
          const rows = db.query(sql).all();
          return { sql, count: rows.length, rows: (rows as unknown[]).slice(0, 50) };
        } catch (err) {
          return { error: err instanceof Error ? err.message : String(err) };
        }
      },
    }),

    context_curate: tool({
      description:
        "Manage the agent's own context, memory, and intelligence. Actions: memory_search, memory_forget, cost_review, skills_list, skills_update, sessions_list, sessions_summarize, sessions_archive, sessions_label, policy_inspect, index_status, prune_stale, conventions_list.",
      parameters: z.object({
        action: z.enum([
          "memory_search", "memory_forget",
          "cost_review",
          "skills_list", "skills_update",
          "sessions_list", "sessions_summarize", "sessions_archive", "sessions_label",
          "policy_inspect",
          "index_status", "prune_stale",
          "conventions_list",
        ]).describe("The curation action to perform"),
        query: z.string().optional().describe("Search query or identifier"),
        data: z.record(z.unknown()).optional().describe("Action-specific data"),
      }),
      execute: async ({ action, query, data }) => {
        switch (action) {
          case "memory_search": {
            const results = db
              .query<{ detail: string; created_at: number }, [number]>(
                `SELECT detail, created_at FROM events WHERE type = 'memory'
                 AND detail LIKE '%' || ? || '%' ORDER BY created_at DESC LIMIT 20`,
              )
              .all(20);
            return { action, count: results.length, results };
          }

          case "memory_forget": {
            if (!query) return { error: "Provide query to match memories to forget" };
            const deleted = db
              .prepare("DELETE FROM events WHERE type = 'memory' AND detail LIKE '%' || ? || '%'")
              .run(query);
            return { action, deleted: (deleted as any).changes ?? 0 };
          }

          case "cost_review": {
            const today = db.query<{ total: number; sessions: number; tokens_in: number; tokens_out: number }, []>(
              "SELECT * FROM v_cost_today",
            ).get();
            const recent = db.query<{ day: string; agent_name: string | null; turns: number; total_cost: number }, []>(
              "SELECT * FROM v_cost_summary ORDER BY day DESC LIMIT 7",
            ).all();
            return { action, today, last_7_days: recent };
          }

          case "skills_list": {
            const skills = db.query<{ name: string; description: string; confidence: number; instructions: string | null }, []>(
              "SELECT name, description, confidence, instructions FROM skills ORDER BY confidence DESC",
            ).all();
            return { action, count: skills.length, skills };
          }

          case "skills_update": {
            const name = query ?? (data as any)?.name;
            const updates = data as Record<string, unknown> | undefined;
            if (!name || !updates) return { error: "Provide query (skill name) and data (updates)" };
            const existing = db.query<{ id: string }, [string]>("SELECT id FROM skills WHERE name = ?").get(name);
            if (!existing) return { error: `Skill not found: ${name}` };
            if (updates.description) db.query("UPDATE skills SET description = ? WHERE id = ?").run(String(updates.description), existing.id);
            if (updates.instructions) db.query("UPDATE skills SET instructions = ? WHERE id = ?").run(String(updates.instructions), existing.id);
            if (updates.confidence != null) db.query("UPDATE skills SET confidence = ? WHERE id = ?").run(Number(updates.confidence), existing.id);
            db.query("UPDATE skills SET updated_at = unixepoch() WHERE id = ?").run(existing.id);
            return { action, updated: name };
          }

          case "sessions_list": {
            const sessions = db.query<{ id: string; label: string | null; agent_name: string | null; is_active: number; created_at: number }, []>(
              "SELECT id, label, agent_name, is_active, created_at FROM sessions ORDER BY created_at DESC LIMIT 20",
            ).all();
            return { action, count: sessions.length, sessions };
          }

          case "sessions_summarize": {
            if (!query) return { error: "Provide session ID prefix" };
            const session = db.query<{ id: string }, [string]>(
              "SELECT id FROM sessions WHERE id LIKE ? || '%'",
            ).get(query);
            if (!session) return { error: `Session not found: ${query}` };
            db.query("INSERT INTO work_queue (type, session_id, created_at) VALUES ('summarize', ?, unixepoch())").run(session.id);
            return { action, queued: "summarize", session_id: session.id };
          }

          case "sessions_archive": {
            if (!query) return { error: "Provide session ID prefix" };
            const session = db.query<{ id: string }, [string]>(
              "SELECT id FROM sessions WHERE id LIKE ? || '%'",
            ).get(query);
            if (!session) return { error: `Session not found: ${query}` };
            db.query("UPDATE sessions SET is_active = 0, last_active_at = unixepoch() WHERE id = ?").run(session.id);
            return { action, archived: session.id };
          }

          case "sessions_label": {
            if (!query) return { error: "Provide session ID prefix" };
            const label = (data as any)?.label ?? "unlabeled";
            const session = db.query<{ id: string }, [string]>(
              "SELECT id FROM sessions WHERE id LIKE ? || '%'",
            ).get(query);
            if (!session) return { error: `Session not found: ${query}` };
            db.query("UPDATE sessions SET label = ? WHERE id = ?").run(String(label), session.id);
            return { action, labeled: session.id, label };
          }

          case "policy_inspect": {
            const policies = db.query<{ name: string; effect: string; description: string; conditions: string | null; enabled: number }, []>(
              "SELECT name, effect, description, conditions, enabled FROM policies",
            ).all();
            return { action, count: policies.length, policies };
          }

          case "index_status": {
            const chunks = db.query<{ cnt: number }, []>("SELECT COUNT(*) as cnt FROM code_chunks").get()?.cnt ?? 0;
            const files = db.query<{ cnt: number }, []>("SELECT COUNT(*) as cnt FROM file_tree").get()?.cnt ?? 0;
            const embedded = db.query<{ cnt: number }, []>("SELECT COUNT(*) as cnt FROM code_chunks WHERE embedding IS NOT NULL").get()?.cnt ?? 0;
            const symbols = db.query<{ cnt: number }, []>("SELECT COUNT(*) as cnt FROM code_chunks WHERE symbol_name IS NOT NULL").get()?.cnt ?? 0;
            const pending = db.query<{ cnt: number }, []>("SELECT COUNT(*) as cnt FROM work_queue WHERE type = 'embed' AND processed_at IS NULL").get()?.cnt ?? 0;
            const langs = db.query<{ language: string; cnt: number }, []>(
              "SELECT language, COUNT(*) as cnt FROM code_chunks WHERE language IS NOT NULL GROUP BY language ORDER BY cnt DESC",
            ).all();
            return { action, chunks, files, embedded, symbols, pending_embeds: pending, languages: langs };
          }

          case "prune_stale": {
            const stale = db.query<{ cnt: number }, []>(
              "SELECT COUNT(*) as cnt FROM code_chunks WHERE updated_at < unixepoch() - 86400 * 7",
            ).get()?.cnt ?? 0;
            if (stale > 0) {
              db.exec("DELETE FROM code_chunks WHERE updated_at < unixepoch() - 86400 * 7");
            }
            return { action, pruned: stale };
          }

          case "conventions_list": {
            const convs = db.query<{ name: string; category: string; description: string; confidence: number }, []>(
              "SELECT name, category, description, confidence FROM conventions ORDER BY confidence DESC",
            ).all();
            return { action, count: convs.length, conventions: convs };
          }

          default:
            return { error: `Unknown action: ${action}` };
        }
      },
    }),

    run_maintenance: tool({
      description: "Trigger work queue processing (summarize, label, embed pending items).",
      parameters: z.object({}),
      execute: async () => {
        const pending = db.query<{ cnt: number }, []>(
          "SELECT COUNT(*) as cnt FROM work_queue WHERE processed_at IS NULL",
        ).get()?.cnt ?? 0;
        const byType = db.query<{ type: string; cnt: number }, []>(
          "SELECT type, COUNT(*) as cnt FROM work_queue WHERE processed_at IS NULL GROUP BY type",
        ).all();
        return { pending_items: pending, by_type: byType };
      },
    }),
  };

  if (!filter) return all;

  const filtered: Record<string, ReturnType<typeof tool>> = {};
  for (const name of filter) {
    if (all[name]) filtered[name] = all[name];
  }
  return filtered;
}
