import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import type { MCPServerHandle } from "../types.js";
import { SidecarClient } from "./client.js";
import { runCodeSearch } from "./tools.js";

type ToolResult = { content: [{ type: "text"; text: string }]; isError?: true };

function toolErr(message: string): ToolResult {
  return { content: [{ type: "text", text: message }], isError: true };
}

function toolOk(text: string): ToolResult {
  return { content: [{ type: "text", text }] };
}

function wrapToolHandler(
  name: string,
  fn: (args: any) => string | Promise<string>,
): (args: any) => Promise<ToolResult> {
  return async (args) => {
    try {
      return toolOk(await fn(args));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return toolErr(`${name} failed: ${msg}`);
    }
  };
}

/**
 * MCP server that proxies code search, policy evaluation, and audit queries
 * to a running `mp serve` HTTP instance.
 */
export function createSidecarServer(baseUrl: string): MCPServerHandle {
  const client = new SidecarClient(baseUrl);
  const server = new McpServer({
    name: "mp-sidecar",
    version: "0.1.0",
  });

  server.registerTool(
    "code_search",
    {
      description: "Search the indexed repository via the moneypenny HTTP API.",
      inputSchema: {
        query: z.string(),
        limit: z.number().int().positive().max(100).optional(),
        languages: z.array(z.string()).optional(),
        paths: z.array(z.string()).optional(),
      },
    },
    wrapToolHandler("code_search", async (args) => {
      if (!(await client.health())) {
        return "moneypenny server not reachable. Start with: mp serve";
      }
      const { text } = await runCodeSearch(client, args);
      return text;
    }),
  );

  server.registerTool(
    "policy_evaluate",
    {
      description: "Evaluate governance policies for an actor, action, and resource.",
      inputSchema: {
        actor: z.string(),
        action: z.string(),
        resource: z.string(),
        denyByDefault: z.boolean().optional(),
        sessionId: z.string().optional(),
      },
    },
    wrapToolHandler("policy_evaluate", async (args) => {
      if (!(await client.health())) {
        return JSON.stringify({ error: "moneypenny server not reachable" });
      }
      const r = await client.evaluatePolicy(args);
      return JSON.stringify(r ?? { error: "no response" }, null, 2);
    }),
  );

  server.registerTool(
    "audit_query",
    {
      description: "Read recent audit events from the moneypenny event log.",
      inputSchema: {
        limit: z.number().int().positive().max(500).optional(),
        type: z.string().optional(),
        sessionId: z.string().optional(),
      },
    },
    wrapToolHandler("audit_query", async (args) => {
      if (!(await client.health())) {
        return JSON.stringify([]);
      }
      const rows = await client.queryAuditLog({
        limit: args.limit ?? 50,
        type: args.type,
        sessionId: args.sessionId,
      });
      return JSON.stringify(rows, null, 2);
    }),
  );

  let transport: StdioServerTransport | null = null;

  return {
    async serveStdio() {
      transport = new StdioServerTransport();
      await server.connect(transport);
    },
    async close() {
      await server.close();
      transport = null;
    },
  };
}
