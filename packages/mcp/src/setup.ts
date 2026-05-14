import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

function ensureDir(path: string): void {
  mkdirSync(path, { recursive: true });
}

function readJson(path: string): Record<string, unknown> {
  if (!existsSync(path)) return {};
  try {
    const v = JSON.parse(readFileSync(path, "utf8")) as unknown;
    if (typeof v === "object" && v !== null && !Array.isArray(v)) {
      return v as Record<string, unknown>;
    }
  } catch {
    /* */
  }
  return {};
}

function mergeMcpServers(root: Record<string, unknown>, servers: Record<string, unknown>): void {
  const cur = (root.mcpServers as Record<string, unknown> | undefined) ?? {};
  root.mcpServers = { ...cur, ...servers };
}

/** Writes or merges `.cursor/mcp.json` for the Cursor IDE. */
export function writeCursorConfig(repoPath: string): void {
  const dir = join(repoPath, ".cursor");
  ensureDir(dir);
  const fp = join(dir, "mcp.json");
  const root = readJson(fp);
  mergeMcpServers(root, {
    swe: {
      command: "swe",
      args: ["mcp", "--repo", repoPath],
    },
  });
  writeFileSync(fp, `${JSON.stringify(root, null, 2)}\n`);
}

function claudeDesktopConfigPath(): string | null {
  if (process.platform === "darwin") {
    const home = process.env.HOME;
    if (!home) return null;
    return join(home, "Library/Application Support/Claude/claude_desktop_config.json");
  }
  if (process.platform === "win32") {
    const appData = process.env.APPDATA;
    if (!appData) return null;
    return join(appData, "Claude", "claude_desktop_config.json");
  }
  const home = process.env.HOME;
  if (!home) return null;
  return join(home, ".config/Claude/claude_desktop_config.json");
}

/** Merges the swe MCP server entry into Claude Desktop config when the path exists or can be created. */
export function writeClaudeConfig(repoPath: string): void {
  const fp = claudeDesktopConfigPath();
  if (!fp) {
    console.error("[swe] Could not resolve Claude Desktop config path for this platform.");
    return;
  }
  ensureDir(dirname(fp));
  const root = readJson(fp);
  mergeMcpServers(root, {
    swe: {
      command: "swe",
      args: ["mcp", "--repo", repoPath],
    },
  });
  writeFileSync(fp, `${JSON.stringify(root, null, 2)}\n`);
}
