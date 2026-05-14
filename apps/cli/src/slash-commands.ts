import { inferProvider, type ProviderName } from "@swe/loop";
import {
  compactConversation,
  createSession,
  getCurrentTurn,
  getSessionMetrics,
  listSessions,
  setActiveSession,
  type AgentDB,
} from "@swe/db";
import { getIndexStatus, hybridSearch, indexCodebase } from "@swe/search";
import {
  accent,
  muted,
  success,
  Spinner,
  printError,
  printHelp,
  printInfo,
} from "./display.js";
import { availableProviders, resolveConfig } from "./config.js";
import { listAgents, type AgentInfo } from "./session.js";
import { timeAgo } from "./pickers.js";

interface ModelEntry {
  id: string;
  label: string;
  provider: ProviderName;
}

const MODEL_CATALOG: ModelEntry[] = [
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6", provider: "anthropic" },
  { id: "claude-sonnet-4-20250514", label: "Claude Sonnet 4", provider: "anthropic" },
  { id: "claude-opus-4-20250514", label: "Claude Opus 4", provider: "anthropic" },
  { id: "claude-3-5-sonnet-20241022", label: "Claude 3.5 Sonnet", provider: "anthropic" },
  { id: "claude-3-5-haiku-20241022", label: "Claude 3.5 Haiku", provider: "anthropic" },
  { id: "gpt-4o", label: "GPT-4o", provider: "openai" },
  { id: "gpt-4o-mini", label: "GPT-4o Mini", provider: "openai" },
  { id: "o3", label: "o3", provider: "openai" },
  { id: "o4-mini", label: "o4-mini", provider: "openai" },
  { id: "gemini-2.5-pro", label: "Gemini 2.5 Pro", provider: "google" },
  { id: "gemini-2.5-flash", label: "Gemini 2.5 Flash", provider: "google" },
  { id: "gemini-2.0-flash", label: "Gemini 2.0 Flash", provider: "google" },
];

export interface SlashContext {
  db: AgentDB;
  repoPath: string;
}

export type SlashResult =
  | { switchModel: { model: string; provider: ProviderName } }
  | { newSession: true }
  | void;

export async function handleSlashCommand(
  line: string,
  ctx: SlashContext,
): Promise<SlashResult> {
  const { db, repoPath } = ctx;
  const body = line.slice(1).trim();
  const firstSpace = body.indexOf(" ");
  const cmd = (firstSpace === -1 ? body : body.slice(0, firstSpace)).toLowerCase();
  const argRest = firstSpace === -1 ? "" : body.slice(firstSpace).trim();

  switch (cmd) {
    case "help": {
      printHelp();
      break;
    }
    case "compact": {
      const turn = getCurrentTurn(db);
      const summary =
        argRest.length > 0 ? argRest : "[Compaction via /compact — summarize earlier turns in subsequent context]";
      compactConversation(db, turn, summary);
      printInfo(muted(`  Compaction marker recorded through turn ${String(turn)}.`));
      break;
    }
    case "fresh": {
      return { newSession: true };
    }
    case "sessions": {
      const sessions = listSessions(db);
      if (sessions.length === 0) {
        printInfo("  No sessions yet.");
        break;
      }
      process.stdout.write("\n");
      for (const s of sessions) {
        const label = s.label ?? "(unlabeled)";
        const active = s.id === db.activeSessionId ? accent(" \u2190") : "";
        const info = `${String(s.turns)} turns ${muted("\u00b7")} $${s.costUsd.toFixed(2)} ${muted("\u00b7")} ${timeAgo(s.lastActiveAt)}`;
        process.stdout.write(`  ${label}  ${info}${active}\n`);
      }
      process.stdout.write("\n");
      break;
    }
    case "agents": {
      const agents = listAgents(repoPath);
      if (agents.length === 0) {
        printInfo("  No agents.");
        break;
      }
      process.stdout.write("\n");
      for (const a of agents) {
        const bpLabel = a.blueprintDescription ?? a.blueprintName ?? "unknown";
        process.stdout.write(`  ${a.name}  ${muted(bpLabel)}\n`);
      }
      process.stdout.write("\n");
      break;
    }
    case "search": {
      const q = argRest;
      if (!q) {
        printError("Usage: /search <query>");
        break;
      }
      const hits = hybridSearch(db, q, { limit: 15 });
      if (hits.length === 0) {
        printInfo("  No hits.");
        break;
      }
      for (const r of hits) {
        process.stdout.write(
          `\n  ${accent(r.path)}:${String(r.startLine)}-${String(r.endLine)} ${muted(`(score: ${r.score.toFixed(2)})`)}\n`,
        );
        process.stdout.write(`  ${muted("\u2500".repeat(40))}\n`);
        const lines = r.chunkText.split("\n");
        for (const ln of lines.slice(0, 6)) process.stdout.write(`  ${ln}\n`);
        if (lines.length > 6) process.stdout.write(`${muted("  ...")}\n`);
      }
      process.stdout.write("\n");
      break;
    }
    case "index": {
      const idxSpinner = new Spinner();
      idxSpinner.start("Indexing...");
      const result = indexCodebase(db, repoPath);
      idxSpinner.stop();
      process.stdout.write(
        `  ${success("\u2714")} Indexed ${String(result.filesScanned)} files, ${String(result.chunksCreated)} chunks in ${(result.elapsedMs / 1000).toFixed(1)}s\n`,
      );
      break;
    }
    case "cost": {
      const m = getSessionMetrics(db);
      const parts = [
        `${String(m.totalTurns)} turns`,
        `${String(m.totalInputTokens)} in`,
        `${String(m.totalOutputTokens)} out`,
        `${String(m.totalToolCalls)} tools`,
        `$${m.totalCostUsd.toFixed(4)}`,
      ];
      printInfo(`  ${parts.join(` ${muted("\u00b7")} `)}`);
      break;
    }
    case "status": {
      const st = getIndexStatus(db);
      const m = getSessionMetrics(db);
      printInfo(`  ${muted("index")}    ${String(st.totalFiles)} files, ${String(st.totalChunks)} chunks`);
      printInfo(`  ${muted("session")}  ${String(m.totalTurns)} turns, $${m.totalCostUsd.toFixed(4)}`);
      break;
    }
    case "model": {
      return handleModelCommand(argRest);
    }
    default:
      printError(`Unknown command /${cmd}. Type /help.`);
      break;
  }
}

function handleModelCommand(
  argRest: string,
): { switchModel: { model: string; provider: ProviderName } } | void {
  const available = availableProviders();

  if (argRest) {
    const exact = MODEL_CATALOG.find((m) => m.id === argRest || m.label.toLowerCase() === argRest.toLowerCase());
    if (exact) {
      if (!available.includes(exact.provider)) {
        printError(`No API key configured for ${exact.provider}. Run \`swe config set ${exact.provider}_api_key <key>\``);
        return;
      }
      printInfo(`  Switching to ${accent(exact.label)} ${muted(`(${exact.id})`)}`);
      return { switchModel: { model: exact.id, provider: exact.provider } };
    }
    const byProvider = MODEL_CATALOG.filter((m) => m.provider === argRest.toLowerCase());
    if (byProvider.length > 0 && available.includes(argRest.toLowerCase() as ProviderName)) {
      printInfo(`  Switching to ${accent(byProvider[0]!.label)} ${muted(`(${byProvider[0]!.id})`)}`);
      return { switchModel: { model: byProvider[0]!.id, provider: byProvider[0]!.provider } };
    }
    const inferred = inferProvider(argRest);
    printInfo(`  Switching to ${accent(argRest)}`);
    return { switchModel: { model: argRest, provider: inferred } };
  }

  const filtered = MODEL_CATALOG.filter((m) => available.includes(m.provider));
  if (filtered.length === 0) {
    printError("No API keys configured. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GOOGLE_API_KEY.");
    return;
  }

  process.stdout.write("\n");
  let currentProvider: ProviderName | null = null;
  for (let i = 0; i < filtered.length; i++) {
    const entry = filtered[i]!;
    if (entry.provider !== currentProvider) {
      currentProvider = entry.provider;
      process.stdout.write(`  ${accent(currentProvider)}\n`);
    }
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${entry.label} ${muted(`(${entry.id})`)}\n`);
  }
  process.stdout.write(`\n  ${muted("Usage: /model <name|number>")}\n\n`);
}
