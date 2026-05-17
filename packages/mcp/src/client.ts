import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type { Database } from "bun:sqlite";

export interface McpServerConfig {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

interface ConnectedServer {
  config: McpServerConfig;
  client: Client;
  transport: StdioClientTransport;
  tools: Array<{
    name: string;
    description?: string;
    inputSchema: Record<string, unknown>;
  }>;
}

export class McpClientManager {
  private servers = new Map<string, ConnectedServer>();

  async connect(config: McpServerConfig): Promise<string[]> {
    if (this.servers.has(config.name)) {
      await this.disconnect(config.name);
    }

    const transport = new StdioClientTransport({
      command: config.command,
      args: config.args,
      env: { ...process.env, ...(config.env ?? {}) } as Record<string, string>,
    });

    const client = new Client(
      { name: "moneypenny", version: "0.1.0" },
      { capabilities: {} },
    );

    await client.connect(transport);

    const { tools } = await client.listTools();

    const connected: ConnectedServer = {
      config,
      client,
      transport,
      tools: tools.map((t) => ({
        name: t.name,
        description: t.description,
        inputSchema: t.inputSchema as Record<string, unknown>,
      })),
    };

    this.servers.set(config.name, connected);
    return tools.map((t) => `${config.name}/${t.name}`);
  }

  async disconnect(name: string): Promise<void> {
    const server = this.servers.get(name);
    if (!server) return;
    try {
      await server.client.close();
    } catch {}
    this.servers.delete(name);
  }

  async disconnectAll(): Promise<void> {
    for (const name of [...this.servers.keys()]) {
      await this.disconnect(name);
    }
  }

  async callTool(
    qualifiedName: string,
    args: Record<string, unknown>,
  ): Promise<unknown> {
    const [serverName, ...toolParts] = qualifiedName.split("/");
    const toolName = toolParts.join("/");

    const server = this.servers.get(serverName);
    if (!server) throw new Error(`MCP server '${serverName}' not connected`);

    const result = await server.client.callTool({
      name: toolName,
      arguments: args,
    });

    return result.content;
  }

  listTools(): Array<{
    server: string;
    name: string;
    qualifiedName: string;
    description?: string;
    inputSchema: Record<string, unknown>;
  }> {
    const result: Array<{
      server: string;
      name: string;
      qualifiedName: string;
      description?: string;
      inputSchema: Record<string, unknown>;
    }> = [];

    for (const [serverName, server] of this.servers) {
      for (const tool of server.tools) {
        result.push({
          server: serverName,
          name: tool.name,
          qualifiedName: `${serverName}/${tool.name}`,
          description: tool.description,
          inputSchema: tool.inputSchema,
        });
      }
    }
    return result;
  }

  listServers(): Array<{
    name: string;
    command: string;
    toolCount: number;
  }> {
    return [...this.servers.entries()].map(([name, server]) => ({
      name,
      command: server.config.command,
      toolCount: server.tools.length,
    }));
  }

  static loadFromConfig(db: Database): McpServerConfig[] {
    const row = db
      .query<{ value: string }, [string]>(
        "SELECT value FROM config WHERE key = ?",
      )
      .get("mcp.servers");

    if (!row) return [];
    try {
      return JSON.parse(row.value);
    } catch {
      return [];
    }
  }
}
