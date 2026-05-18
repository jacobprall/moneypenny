import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourcesRequestSchema,
  ReadResourceRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { zodToJsonSchema } from "zod-to-json-schema";
import * as actions from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

type McpTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (args: Record<string, unknown>) => Promise<string>;
};

function defineTool<T extends z.ZodRawShape>(def: {
  name: string;
  description: string;
  schema: z.ZodObject<T>;
  handler: (args: z.infer<z.ZodObject<T>>) => Promise<unknown>;
}): McpTool {
  return {
    name: def.name,
    description: def.description,
    inputSchema: zodToJsonSchema(def.schema, { target: "openApi3" }) as Record<
      string,
      unknown
    >,
    handler: async (raw) => {
      const parsed = def.schema.safeParse(raw);
      if (!parsed.success) return JSON.stringify({ error: parsed.error.flatten() });
      return JSON.stringify(await def.handler(parsed.data));
    },
  };
}

function buildTools(ctx: ActionContext): McpTool[] {
  return [
    defineTool({
      name: "launch_agent",
      description: "Launch an agent session from a blueprint",
      schema: z.object({
        blueprint: z.string(),
        task: z.string(),
        cwd: z.string().optional(),
        label: z.string().optional(),
      }),
      handler: (a) => actions.launchAgent(ctx, a),
    }),
    defineTool({
      name: "inject_message",
      description: "Send a message to a session",
      schema: z.object({ sessionId: z.string(), content: z.string() }),
      handler: async (a) => {
        await actions.injectMessage(ctx, a.sessionId, a.content);
        return { ok: true };
      },
    }),
    defineTool({
      name: "list_sessions",
      description: "List sessions",
      schema: z.object({
        status: z.string().optional(),
        blueprint: z.string().optional(),
        label: z.string().optional(),
        cursor: z.number().optional(),
        limit: z.number().optional(),
      }),
      handler: (a) =>
        actions.listSessions(ctx, {
          status: a.status,
          blueprint: a.blueprint,
          label: a.label,
          cursor: a.cursor ?? null,
          limit: a.limit,
        }),
    }),
    defineTool({
      name: "get_session",
      description: "Get session with recent runs",
      schema: z.object({ id: z.string() }),
      handler: (a) => actions.getSessionDetail(ctx, a.id),
    }),
    defineTool({
      name: "pause_session",
      schema: z.object({ sessionId: z.string() }),
      description: "Pause a running session",
      handler: async (a) => {
        await actions.pauseSession(ctx, a.sessionId);
        return { ok: true };
      },
    }),
    defineTool({
      name: "resume_session",
      schema: z.object({ sessionId: z.string() }),
      description: "Resume a paused session",
      handler: async (a) => {
        await actions.resumeSession(ctx, a.sessionId);
        return { ok: true };
      },
    }),
    defineTool({
      name: "archive_session",
      schema: z.object({ sessionId: z.string() }),
      description: "Archive a session",
      handler: async (a) => {
        actions.archiveSession(ctx, a.sessionId);
        return { ok: true };
      },
    }),
    defineTool({
      name: "list_blueprints",
      schema: z.object({}),
      description: "List blueprints",
      handler: () => actions.listBlueprints(ctx),
    }),
    defineTool({
      name: "list_ideas",
      schema: z.object({
        status: z.string().optional(),
        tags: z.string().optional(),
        cwd: z.string().optional(),
      }),
      description: "List ideas",
      handler: (a) => actions.listIdeas(ctx, a),
    }),
    defineTool({
      name: "create_idea",
      schema: z.object({
        filename: z.string(),
        body: z.string(),
        frontmatter: z.record(z.unknown()),
      }),
      description: "Create an idea file",
      handler: (a) => actions.createIdea(ctx, a),
    }),
    defineTool({
      name: "search_code",
      schema: z.object({ q: z.string(), limit: z.number().optional() }),
      description: "Hybrid code search",
      handler: (a) => actions.searchCode(ctx, a.q, a.limit),
    }),
    defineTool({
      name: "search_messages",
      schema: z.object({ q: z.string() }),
      description: "FTS search messages",
      handler: (a) => Promise.resolve(actions.searchMessages(ctx, a.q)),
    }),
    defineTool({
      name: "list_skills",
      schema: z.object({}),
      description: "List skills",
      handler: () => actions.listSkills(ctx),
    }),
    defineTool({
      name: "list_conventions",
      schema: z.object({}),
      description: "List conventions",
      handler: () => actions.listConventions(ctx),
    }),
    defineTool({
      name: "record_pointer",
      schema: z.object({
        sessionId: z.string(),
        key: z.string(),
        phrase: z.string(),
        pinned: z.boolean().optional(),
      }),
      description: "Record a session pointer",
      handler: (a) => actions.recordPointer(ctx, a),
    }),
    defineTool({
      name: "get_health",
      schema: z.object({}),
      description: "System health",
      handler: () => actions.getHealth(ctx),
    }),

    // --- Sessions (missing) ---

    defineTool({
      name: "create_session",
      description: "Create a new session",
      schema: z.object({
        blueprint: z.string().optional(),
        cwd: z.string(),
        label: z.string().optional(),
        parentId: z.string().optional(),
        ideaId: z.string().optional(),
        task: z.string().optional(),
      }),
      handler: (a) =>
        actions.createSession(ctx, {
          blueprint: a.blueprint,
          cwd: a.cwd,
          label: a.label ?? null,
          parentId: a.parentId ?? null,
          ideaId: a.ideaId ?? null,
          task: a.task,
        }),
    }),
    defineTool({
      name: "complete_session",
      description: "Mark a session as completed",
      schema: z.object({ sessionId: z.string() }),
      handler: async (a) => {
        actions.completeSession(ctx, a.sessionId);
        return { ok: true };
      },
    }),
    defineTool({
      name: "delete_session",
      description: "Permanently delete a session",
      schema: z.object({ sessionId: z.string() }),
      handler: async (a) => {
        actions.deleteSession(ctx, a.sessionId);
        return { ok: true };
      },
    }),
    defineTool({
      name: "update_session_config",
      description: "Update a session's config (optimistic concurrency via config_version)",
      schema: z.object({
        sessionId: z.string(),
        config: z.string().describe("JSON-encoded config object"),
        expectedVersion: z.number(),
      }),
      handler: async (a) =>
        actions.updateSessionConfig(ctx, a.sessionId, a.config, a.expectedVersion),
    }),

    // --- Messages ---

    defineTool({
      name: "list_messages",
      description: "List messages for a session (cursor-based pagination)",
      schema: z.object({
        sessionId: z.string(),
        cursor: z.number().optional(),
        limit: z.number().optional(),
        direction: z.enum(["before", "after"]).optional(),
      }),
      handler: (a) =>
        actions.listMessagesBySession(ctx, a.sessionId, {
          cursor: a.cursor ?? null,
          limit: a.limit,
          direction: a.direction,
        }),
    }),

    // --- Runs ---

    defineTool({
      name: "list_runs",
      description: "List runs for a session",
      schema: z.object({ sessionId: z.string() }),
      handler: async (a) => actions.listRunsBySession(ctx, a.sessionId),
    }),
    defineTool({
      name: "get_run",
      description: "Get run detail including messages",
      schema: z.object({ id: z.string() }),
      handler: async (a) => actions.getRunDetail(ctx, a.id),
    }),

    // --- Blueprints (missing) ---

    defineTool({
      name: "get_blueprint",
      description: "Get a parsed blueprint by name",
      schema: z.object({ name: z.string() }),
      handler: async (a) => actions.getBlueprint(ctx, a.name),
    }),
    defineTool({
      name: "reload_blueprints",
      description: "Force blueprint registry refresh",
      schema: z.object({
        globalDir: z.string(),
        repoDir: z.string().optional(),
      }),
      handler: async (a) => {
        actions.reloadBlueprints(ctx, { global: a.globalDir, repo: a.repoDir });
        return { ok: true };
      },
    }),

    // --- Ideas (missing) ---

    defineTool({
      name: "get_idea",
      description: "Get a parsed idea by filename",
      schema: z.object({ filename: z.string() }),
      handler: async (a) => actions.getIdea(ctx, a.filename),
    }),
    defineTool({
      name: "delete_idea",
      description: "Delete an idea file",
      schema: z.object({ filename: z.string() }),
      handler: async (a) => {
        await actions.deleteIdea(ctx, a.filename);
        return { ok: true };
      },
    }),

    // --- Agents (missing) ---

    defineTool({
      name: "agent_status",
      description: "Get runtime agent pool status",
      schema: z.object({}),
      handler: async () => actions.getAgentStatus(ctx),
    }),
    defineTool({
      name: "kill_agent",
      description: "Abort the current run for a session",
      schema: z.object({ sessionId: z.string() }),
      handler: async (a) => {
        await actions.killAgent(ctx, a.sessionId);
        return { ok: true };
      },
    }),

    // --- Tools ---

    defineTool({
      name: "list_tools",
      description: "List registered tools with schemas and permissions",
      schema: z.object({}),
      handler: async () => actions.listTools(ctx),
    }),

    // --- Code (missing) ---

    defineTool({
      name: "read_code_file",
      description: "Read a file by cwd-relative path",
      schema: z.object({
        cwd: z.string(),
        path: z.string().describe("Relative path from cwd"),
      }),
      handler: async (a) => {
        const content = await actions.readCodeFile(ctx, a.cwd, a.path);
        return content ?? { error: "file not found" };
      },
    }),
    defineTool({
      name: "trigger_reindex",
      description: "Queue a full code reindex",
      schema: z.object({}),
      handler: async () => actions.triggerReindex(ctx),
    }),

    // --- Files ---

    defineTool({
      name: "list_directory",
      description: "List directory contents",
      schema: z.object({
        cwd: z.string(),
        path: z.string().describe("Relative path from cwd"),
      }),
      handler: (a) => actions.listDirectory(ctx, a.cwd, a.path),
    }),
    defineTool({
      name: "stat_file",
      description: "Get file metadata (size, isDir, mtime)",
      schema: z.object({
        cwd: z.string(),
        path: z.string().describe("Relative path from cwd"),
      }),
      handler: (a) => actions.statFile(ctx, a.cwd, a.path),
    }),
    defineTool({
      name: "read_file",
      description: "Read a text file's content",
      schema: z.object({
        cwd: z.string(),
        path: z.string().describe("Relative path from cwd"),
      }),
      handler: async (a) => ({ content: await actions.readFileText(ctx, a.cwd, a.path) }),
    }),

    // --- Knowledge (missing) ---

    defineTool({
      name: "list_pointers",
      description: "List session pointers",
      schema: z.object({
        sessionId: z.string().optional(),
        pinnedOnly: z.boolean().optional(),
      }),
      handler: async (a) =>
        actions.listPointers(ctx, {
          sessionId: a.sessionId,
          pinnedOnly: a.pinnedOnly,
        }),
    }),

    // --- Events ---

    defineTool({
      name: "list_events",
      description: "List events (cursor-based, filterable by type/session)",
      schema: z.object({
        afterId: z.number().optional(),
        limit: z.number().optional(),
        sessionId: z.string().optional(),
        type: z.string().optional(),
      }),
      handler: (a) =>
        actions.listEvents(ctx, {
          afterId: a.afterId ?? null,
          limit: a.limit,
          sessionId: a.sessionId,
          type: a.type,
        }),
    }),

    // --- Tabs ---

    defineTool({
      name: "list_tabs",
      description: "List open tabs",
      schema: z.object({}),
      handler: async () => actions.listTabs(ctx),
    }),
    defineTool({
      name: "create_tab",
      description: "Open a new tab",
      schema: z.object({
        kind: z.string(),
        sessionId: z.string().optional(),
        label: z.string().optional(),
        position: z.number().optional(),
        active: z.boolean().optional(),
      }),
      handler: (a) =>
        actions.openTab(ctx, {
          kind: a.kind,
          sessionId: a.sessionId ?? null,
          label: a.label ?? null,
          position: a.position,
          active: a.active,
        }),
    }),
    defineTool({
      name: "patch_tab",
      description: "Update tab position, label, or active state",
      schema: z.object({
        id: z.string(),
        position: z.number().optional(),
        label: z.string().optional(),
        active: z.boolean().optional(),
      }),
      handler: async (a) => {
        actions.patchTab(ctx, {
          id: a.id,
          position: a.position,
          label: a.label ?? undefined,
          active: a.active,
        });
        return { ok: true };
      },
    }),
    defineTool({
      name: "delete_tab",
      description: "Close a tab",
      schema: z.object({ id: z.string() }),
      handler: async (a) => {
        actions.closeTab(ctx, a.id);
        return { ok: true };
      },
    }),

    // --- System (missing) ---

    defineTool({
      name: "get_config",
      description: "Read system config key-value pairs",
      schema: z.object({}),
      handler: async () => actions.getSystemConfig(ctx),
    }),
    defineTool({
      name: "set_config",
      description: "Update system config key-value pairs",
      schema: z.object({
        kv: z.record(z.string()).describe("Key-value pairs to set"),
      }),
      handler: async (a) => {
        actions.setSystemConfig(ctx, a.kv);
        return { ok: true };
      },
    }),
  ];
}

export function createMcpServer(ctx: ActionContext): Server {
  const server = new Server(
    { name: "moneypenny", version: "0.4.0" },
    { capabilities: { tools: {}, resources: {} } },
  );

  const tools = buildTools(ctx);
  const toolMap = new Map(tools.map((t) => [t.name, t]));

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: tools.map((t) => ({
      name: t.name,
      description: t.description,
      inputSchema: t.inputSchema,
    })),
  }));

  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const t = toolMap.get(request.params.name);
    if (!t) {
      return {
        content: [{ type: "text", text: `Unknown tool: ${request.params.name}` }],
        isError: true,
      };
    }
    try {
      const result = await t.handler(
        (request.params.arguments as Record<string, unknown>) ?? {},
      );
      return { content: [{ type: "text", text: result }] };
    } catch (err) {
      return {
        content: [
          { type: "text", text: err instanceof Error ? err.message : String(err) },
        ],
        isError: true,
      };
    }
  });

  server.setRequestHandler(ListResourcesRequestSchema, async () => ({
    resources: [
      {
        uri: "moneypenny://health",
        name: "System Health",
        description: "v_health + pool from runtime",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://cost-today",
        name: "Today's Cost",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://sessions",
        name: "Sessions (recent)",
        mimeType: "application/json",
      },
    ],
  }));

  server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
    const uri = request.params.uri;
    if (uri === "moneypenny://health") {
      const h = actions.getHealth(ctx);
      return {
        contents: [{ uri, mimeType: "application/json", text: JSON.stringify(h) }],
      };
    }
    if (uri === "moneypenny://cost-today") {
      const row = ctx.readDb
        .query<
          { total: number; sessions: number; tokens_in: number; tokens_out: number },
          []
        >("SELECT * FROM v_cost_today")
        .get();
      return {
        contents: [
          { uri, mimeType: "application/json", text: JSON.stringify(row ?? {}) },
        ],
      };
    }
    if (uri === "moneypenny://sessions") {
      const rows = actions.listSessions(ctx, { limit: 20 });
      return {
        contents: [
          { uri, mimeType: "application/json", text: JSON.stringify(rows.items) },
        ],
      };
    }
    throw new Error(`Unknown resource: ${uri}`);
  });

  return server;
}

export async function startStdioServer(ctx: ActionContext): Promise<void> {
  const server = createMcpServer(ctx);
  const transport = new StdioServerTransport();
  await server.connect(transport);
}
