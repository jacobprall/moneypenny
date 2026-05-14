import { Command } from "commander";
import * as path from "node:path";
import { writeClaudeConfig, writeCursorConfig } from "@swe/mcp";

export const setupCommand = new Command("setup")
  .description("Write IDE MCP configuration for this repository")
  .argument("<target>", "cursor | claude")
  .option("--repo <path>", "Repository path", process.cwd())
  .action((target: string, opts: { repo: string }) => {
    const repoPath = path.resolve(opts.repo);
    const t = target.toLowerCase();
    try {
      if (t === "cursor") {
        writeCursorConfig(repoPath);
        process.stdout.write(`Wrote Cursor MCP config under ${path.join(repoPath, ".cursor", "mcp.json")}\n`);
        return;
      }
      if (t === "claude") {
        writeClaudeConfig(repoPath);
        process.stdout.write("Merged swe entry into Claude Desktop config.\n");
        return;
      }
      process.stderr.write(`Unknown target "${target}". Use: cursor | claude\n`);
      process.exitCode = 1;
    } catch (e) {
      process.stderr.write(`Error writing config: ${e instanceof Error ? e.message : String(e)}\n`);
      process.exitCode = 1;
    }
  });
