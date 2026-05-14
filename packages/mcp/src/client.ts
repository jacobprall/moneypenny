/**
 * MCP Client Manager — spawns external MCP servers as subprocesses and
 * registers their tools on a {@link ToolRegistry}.
 *
 * Tools are namespaced as `mcp__<server>__<tool_name>`.
 */

import { existsSync } from "fs";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { z } from "zod";
import type { ToolContext, ToolDefinition, ToolRegistry } from "@swe/tools";

export interface McpServerEntry {
  command: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
}

export type McpServersConfig = Record<string, McpServerEntry>;

interface ConnectedServer {
  name: string;
  client: Client;
  transport: StdioClientTransport;
  tools: Map<string, { description: string; inputSchema: Record<string, unknown> }>;
}

function interpolateEnv(value: string): string {
  return value.replace(/\$\{([^}]+)\}/g, (_, key) => process.env[key] ?? "");
}

function resolveEnv(env: Record<string, string> | undefined): Record<string, string> {
  if (!env) return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(env)) {
    out[k] = interpolateEnv(v);
  }
  return out;
}

const looseObjectSchema = z.record(z.string(), z.unknown());

export class McpClientManager {
  private servers: ConnectedServer[] = [];

  async loadFromConfig(configPath: string): Promise<void> {
    if (!existsSync(configPath)) {
      console.error(`[mcp] no config at ${configPath}; skipping`);
      return;
    }
    const raw = await Bun.file(configPath).text();
    let config: McpServersConfig;
    try {
      config = JSON.parse(raw) as McpServersConfig;
    } catch (e) {
      console.error(`[mcp] invalid JSON in ${configPath}:`, e instanceof Error ? e.message : e);
      return;
    }
    await this.startAll(config);
  }

  async startAll(config: McpServersConfig): Promise<void> {
    const entries = Object.entries(config);
    if (entries.length === 0) return;

    const results = await Promise.allSettled(entries.map(([name, entry]) => this.startServer(name, entry)));

    for (let i = 0; i < results.length; i++) {
      const result = results[i];
      const name = entries[i]![0];
      if (result?.status === "rejected") {
        console.error(`[mcp] failed to start ${name}:`, result.reason);
      }
    }
  }

  private async startServer(name: string, entry: McpServerEntry): Promise<void> {
    const env = {
      ...process.env,
      ...resolveEnv(entry.env),
    } as Record<string, string>;

    const transport = new StdioClientTransport({
      command: entry.command,
      args: entry.args,
      env,
      cwd: entry.cwd,
      stderr: "pipe",
    });

    const client = new Client({ name: `swe-${name}`, version: "1.0.0" });

    await client.connect(transport);

    const { tools: rawTools } = await client.listTools();
    const toolMap = new Map<string, { description: string; inputSchema: Record<string, unknown> }>();

    for (const t of rawTools) {
      toolMap.set(t.name, {
        description: t.description ?? "",
        inputSchema: (t.inputSchema ?? {}) as Record<string, unknown>,
      });
    }

    this.servers.push({ name, client, transport, tools: toolMap });
    console.error(`[mcp] ${name}: started (${rawTools.length} tools)`);
  }

  /** Builds {@link ToolDefinition} entries for all connected MCP tools. */
  listToolDefinitions(): ToolDefinition[] {
    const out: ToolDefinition[] = [];
    for (const server of this.servers) {
      for (const [toolName, meta] of server.tools) {
        const qualifiedName = `mcp__${server.name}__${toolName}`;
        const serverRef = server;
        const originalName = toolName;

        out.push({
          name: qualifiedName,
          description: meta.description || `MCP tool ${originalName} from ${server.name}`,
          inputSchema: looseObjectSchema,
          execute: async (input, _ctx: ToolContext) => {
            const args = typeof input === "object" && input !== null ? (input as Record<string, unknown>) : {};
            const result = await serverRef.client.callTool({
              name: originalName,
              arguments: args,
            });

            if ("content" in result && Array.isArray(result.content)) {
              const textParts = result.content
                .filter((c: { type: string }) => c.type === "text")
                .map((c: { type: string; text?: string }) => c.text ?? "");
              return textParts.join("\n");
            }

            return JSON.stringify(result);
          },
        });
      }
    }
    return out;
  }

  /** Registers every connected MCP tool on the given registry. */
  registerWithRegistry(registry: ToolRegistry): void {
    for (const def of this.listToolDefinitions()) {
      registry.register(def);
    }
  }

  getServerNames(): string[] {
    return this.servers.map((s) => s.name);
  }

  getToolCount(): number {
    return this.servers.reduce((sum, s) => sum + s.tools.size, 0);
  }

  async shutdown(): Promise<void> {
    const closeOps = this.servers.map(async (s) => {
      try {
        await s.client.close();
      } catch {
        // best-effort
      }
    });
    await Promise.allSettled(closeOps);
    this.servers = [];
  }
}
