import { Command } from "commander";
import * as path from "node:path";
import { existsSync, mkdirSync } from "node:fs";
import { writeClaudeConfig, writeCursorConfig } from "@swe/mcp";
import { accent, chrome, success, printError } from "../display.js";

interface ModelSpec {
  file: string;
  url: string;
  label: string;
}

const MODELS: ModelSpec[] = [
  {
    file: "nomic-embed-text-v1.5.Q8_0.gguf",
    url: "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf",
    label: "Embedding model (nomic-embed-text v1.5)",
  },
  {
    file: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
    label: "Text gen model (Qwen2.5 0.5B Instruct)",
  },
];

function modelsDir(): string {
  return path.join(process.env.HOME ?? "~", ".swe", "models");
}

async function downloadModel(spec: ModelSpec): Promise<boolean> {
  const dir = modelsDir();
  const dest = path.join(dir, spec.file);

  if (existsSync(dest)) {
    process.stdout.write(`  ${success("[OK]")} ${spec.label} — already downloaded\n`);
    return true;
  }

  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }

  process.stdout.write(`  ${chrome("[ ]")} ${spec.label} — downloading...\n`);

  try {
    const resp = await fetch(spec.url, { redirect: "follow" });
    if (!resp.ok) {
      printError(`Failed to download ${spec.file}: HTTP ${resp.status}`);
      return false;
    }

    const totalBytes = Number(resp.headers.get("content-length") ?? 0);
    const reader = resp.body?.getReader();
    if (!reader) {
      printError(`No response body for ${spec.file}`);
      return false;
    }

    const chunks: Uint8Array[] = [];
    let downloaded = 0;

    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      downloaded += value.length;
      if (totalBytes > 0) {
        const pct = Math.round((downloaded / totalBytes) * 100);
        const mb = (downloaded / 1024 / 1024).toFixed(0);
        const totalMb = (totalBytes / 1024 / 1024).toFixed(0);
        process.stdout.write(`\r  ${chrome("[ ]")} ${spec.label} — ${mb}/${totalMb} MB (${pct}%)`);
      }
    }

    const combined = new Uint8Array(downloaded);
    let offset = 0;
    for (const chunk of chunks) {
      combined.set(chunk, offset);
      offset += chunk.length;
    }

    await Bun.write(dest, combined);
    process.stdout.write(`\r  ${success("[OK]")} ${spec.label} — ${(downloaded / 1024 / 1024).toFixed(0)} MB downloaded\n`);
    return true;
  } catch (e) {
    printError(`Download failed for ${spec.file}: ${e instanceof Error ? e.message : String(e)}`);
    return false;
  }
}

const modelsSubcommand = new Command("models")
  .description("Download required GGUF models for embedding and local text generation")
  .action(async () => {
    process.stdout.write(`\n  ${accent("Downloading models to")} ${chrome(modelsDir())}\n\n`);

    let allOk = true;
    for (const spec of MODELS) {
      const ok = await downloadModel(spec);
      if (!ok) allOk = false;
    }

    process.stdout.write("\n");
    if (allOk) {
      process.stdout.write(`  ${success("[OK]")} All models ready.\n\n`);
    } else {
      printError("Some models failed to download. Re-run: swe setup models");
      process.exitCode = 1;
    }
  });

export const setupCommand = new Command("setup")
  .description("Configure IDE integrations and download models")
  .argument("[target]", "cursor | claude | models")
  .option("--repo <path>", "Repository path", process.cwd())
  .action((target: string | undefined, opts: { repo: string }) => {
    if (!target) {
      process.stdout.write("Usage: swe setup <cursor|claude|models>\n");
      return;
    }

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
      process.stderr.write(`Unknown target "${target}". Use: cursor | claude | models\n`);
      process.exitCode = 1;
    } catch (e) {
      process.stderr.write(`Error writing config: ${e instanceof Error ? e.message : String(e)}\n`);
      process.exitCode = 1;
    }
  });

setupCommand.addCommand(modelsSubcommand);
