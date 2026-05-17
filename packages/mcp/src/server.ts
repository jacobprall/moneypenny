import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourcesRequestSchema,
  ReadResourceRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import type { Database } from "bun:sqlite";
import { createToolSet } from "@moneypenny/engine";

interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (args: Record<string, unknown>) => Promise<string>;
}

function extractProperties(t: unknown): Record<string, unknown> {
  try {
    const params = (t as any).parameters;
    if (params?.shape) {
      const props: Record<string, unknown> = {};
      for (const [key, schema] of Object.entries(params.shape)) {
        const desc =
          (schema as any)?._def?.description ??
          (schema as any)?.description;
        const typeName = (schema as any)?._def?.typeName;
        let type = "string";
        if (typeName === "ZodNumber") type = "number";
        if (typeName === "ZodBoolean") type = "boolean";
        props[key] = { type, ...(desc ? { description: desc } : {}) };
      }
      return props;
    }
  } catch {}
  return {};
}

function createMcpToolList(db: Database): McpTool[] {
  const toolSet = createToolSet(db);

  return Object.entries(toolSet).map(([name, t]) => ({
    name,
    description: (t as any).description ?? name,
    inputSchema: {
      type: "object" as const,
      properties: extractProperties(t),
    },
    handler: async (args: Record<string, unknown>) => {
      const execute = (t as any).execute;
      if (!execute) return JSON.stringify({ error: "No execute function" });
      const result = await execute(args);
      return JSON.stringify(result);
    },
  }));
}

export function createMcpServer(db: Database): Server {
  const server = new Server(
    { name: "moneypenny", version: "0.3.0" },
    { capabilities: { tools: {}, resources: {} } },
  );

  const tools = createMcpToolList(db);
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
        content: [
          { type: "text", text: `Unknown tool: ${request.params.name}` },
        ],
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
          {
            type: "text",
            text: `Error: ${err instanceof Error ? err.message : String(err)}`,
          },
        ],
        isError: true,
      };
    }
  });

  server.setRequestHandler(ListResourcesRequestSchema, async () => ({
    resources: [
      {
        uri: "moneypenny://context",
        name: "Agent Context",
        description:
          "Assembled context (previous sessions, skills, conventions, policies)",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://health",
        name: "System Health",
        description: "Database statistics and health metrics",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://cost-today",
        name: "Today's Cost",
        description: "Token usage and cost for today",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://sessions",
        name: "Recent Sessions",
        description: "List of recent sessions with labels and cost",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://conventions",
        name: "Project Conventions",
        description: "Detected and user-defined project conventions",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://skills",
        name: "Learned Skills",
        description: "Skills the agent has learned across sessions",
        mimeType: "application/json",
      },
      {
        uri: "moneypenny://budget",
        name: "Budget Status",
        description: "Current budget usage and limits",
        mimeType: "application/json",
      },
    ],
  }));

  server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
    const uri = request.params.uri;

    if (uri === "moneypenny://context") {
      const row = db
        .query<{ context: string }, []>(
          "SELECT context FROM v_agent_context",
        )
        .get();
      return {
        contents: [
          { uri, mimeType: "application/json", text: row?.context ?? "{}" },
        ],
      };
    }

    if (uri === "moneypenny://health") {
      const row = db
        .query<{ health: string }, []>("SELECT health FROM v_health")
        .get();
      return {
        contents: [
          { uri, mimeType: "application/json", text: row?.health ?? "{}" },
        ],
      };
    }

    if (uri === "moneypenny://cost-today") {
      const row = db
        .query<
          {
            total: number;
            sessions: number;
            tokens_in: number;
            tokens_out: number;
          },
          []
        >("SELECT * FROM v_cost_today")
        .get();
      return {
        contents: [
          {
            uri,
            mimeType: "application/json",
            text: JSON.stringify(row ?? {}),
          },
        ],
      };
    }

    if (uri === "moneypenny://sessions") {
      const sessions = db
        .query<
          {
            id: string;
            label: string | null;
            agent_name: string | null;
            is_active: number;
            created_at: number;
          },
          []
        >(
          "SELECT id, label, agent_name, is_active, created_at FROM sessions ORDER BY created_at DESC LIMIT 20",
        )
        .all();
      return {
        contents: [
          {
            uri,
            mimeType: "application/json",
            text: JSON.stringify(sessions),
          },
        ],
      };
    }

    if (uri === "moneypenny://conventions") {
      const convs = db
        .query<
          { name: string; category: string; description: string; confidence: number },
          []
        >(
          "SELECT name, category, description, confidence FROM conventions WHERE confidence > 0.3 ORDER BY confidence DESC",
        )
        .all();
      return {
        contents: [
          {
            uri,
            mimeType: "application/json",
            text: JSON.stringify(convs),
          },
        ],
      };
    }

    if (uri === "moneypenny://skills") {
      const skills = db
        .query<
          { name: string; description: string; instructions: string | null; confidence: number },
          []
        >(
          "SELECT name, description, instructions, confidence FROM skills WHERE confidence > 0.3 ORDER BY confidence DESC",
        )
        .all();
      return {
        contents: [
          {
            uri,
            mimeType: "application/json",
            text: JSON.stringify(skills),
          },
        ],
      };
    }

    if (uri === "moneypenny://budget") {
      const daily = db
        .query<{ total: number }, []>(
          "SELECT COALESCE(total, 0) as total FROM v_cost_today",
        )
        .get();

      const policyRow = db
        .query<{ conditions: string | null }, [string]>(
          "SELECT conditions FROM policies WHERE name = ?",
        )
        .get("Budget Guard");

      let limits = { maxDailyUsd: 10, maxSessionUsd: 1 };
      if (policyRow?.conditions) {
        try {
          limits = JSON.parse(policyRow.conditions);
        } catch {}
      }

      const budget = {
        daily_spent: daily?.total ?? 0,
        daily_limit: limits.maxDailyUsd,
        daily_remaining: Math.max(0, limits.maxDailyUsd - (daily?.total ?? 0)),
        session_limit: limits.maxSessionUsd,
      };

      return {
        contents: [
          {
            uri,
            mimeType: "application/json",
            text: JSON.stringify(budget),
          },
        ],
      };
    }

    throw new Error(`Unknown resource: ${uri}`);
  });

  return server;
}

export async function startStdioServer(db: Database): Promise<void> {
  const server = createMcpServer(db);
  const transport = new StdioServerTransport();
  await server.connect(transport);
}
