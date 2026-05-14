import { inferProvider, type ProviderName } from "@moneypenny/loop";
import {
  compactConversation,
  getCurrentTurn,
  getSessionMetrics,
  listSessions,
  type AgentDB,
} from "@moneypenny/db";
import { getIndexStatus, hybridSearch, indexCodebase } from "@moneypenny/search";
import {
  accent,
  bold,
  humanTokens,
  muted,
  success,
  Spinner,
  printError,
  printHelp,
  printInfo,
} from "./display.js";
import { availableProviders, writeGlobalConfigKey } from "./config.js";
import { listAgentDefs } from "./session.js";
import { timeAgo } from "./pickers.js";
import { getTheme, setTheme, isThemeName, THEME_NAMES, THEMES } from "./theme.js";

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
  | { switchSession: { sessionId: string } }
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
    case "sessions":
    case "session": {
      return handleSessionCommand(db, argRest);
    }
    case "agents":
    case "agent": {
      return handleAgentCommand(repoPath, db, argRest);
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
    case "summary":
    case "cost":
    case "status": {
      const st = getIndexStatus(db);
      const m = getSessionMetrics(db);
      const sep = muted("·");
      process.stdout.write("\n");
      process.stdout.write(`    ${muted("turns")}    ${String(m.totalTurns)}\n`);
      process.stdout.write(`    ${muted("in")}       ${humanTokens(m.totalInputTokens)} tokens\n`);
      process.stdout.write(`    ${muted("out")}      ${humanTokens(m.totalOutputTokens)} tokens\n`);
      process.stdout.write(`    ${muted("tools")}    ${String(m.totalToolCalls)} calls\n`);
      process.stdout.write(`    ${muted("cost")}     $${m.totalCostUsd.toFixed(4)}\n`);
      process.stdout.write(`    ${muted("index")}    ${String(st.totalFiles)} files ${sep} ${String(st.totalChunks)} chunks\n`);
      process.stdout.write("\n");
      break;
    }
    case "model": {
      return handleModelCommand(argRest);
    }
    case "theme": {
      return handleThemeCommand(argRest);
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
  const filtered = MODEL_CATALOG.filter((m) => available.includes(m.provider));

  if (argRest) {
    if (/^\d+$/.test(argRest)) {
      const num = parseInt(argRest, 10);
      if (filtered.length === 0) {
        printError("No API keys configured. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GOOGLE_API_KEY.");
        return;
      }
      if (num < 1 || num > filtered.length) {
        printError(`Invalid model number. Choose 1\u2013${String(filtered.length)}. Type /model to see the list.`);
        return;
      }
      const picked = filtered[num - 1]!;
      printInfo(`  Switching to ${accent(picked.label)} ${muted(`(${picked.id})`)}`);
      return { switchModel: { model: picked.id, provider: picked.provider } };
    }

    const exact = MODEL_CATALOG.find((m) => m.id === argRest || m.label.toLowerCase() === argRest.toLowerCase());
    if (exact) {
      if (!available.includes(exact.provider)) {
        printError(`No API key configured for ${exact.provider}. Run \`mp config set ${exact.provider}_api_key <key>\``);
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
      process.stdout.write(`  ${bold(currentProvider)}\n`);
    }
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${entry.label} ${muted(`(${entry.id})`)}\n`);
  }
  process.stdout.write(`\n  ${muted("/model <name|#> to switch")}\n\n`);
}

// ── /theme ──────────────────────────────────────────────────────────────

function handleThemeCommand(argRest: string): void {
  if (argRest) {
    const name = argRest.toLowerCase();
    if (!isThemeName(name)) {
      printError(`Unknown theme "${argRest}". Options: ${THEME_NAMES.join(", ")}`);
      return;
    }
    setTheme(name);
    writeGlobalConfigKey("theme", name);
    printInfo(`  Switched to ${accent(THEMES[name]!.label)} theme`);
    return;
  }

  const current = getTheme();
  process.stdout.write("\n");
  for (const name of THEME_NAMES) {
    const t = THEMES[name];
    const marker = t.id === current.id ? accent("●") : muted("○");
    process.stdout.write(`    ${marker} ${t.label}${t.id === current.id ? muted(" (active)") : ""}\n`);
  }
  process.stdout.write(`\n  ${muted("/theme <name> to switch")}\n\n`);
}

// ── /session ─────────────────────────────────────────────────────────────

function handleSessionCommand(
  db: AgentDB,
  argRest: string,
): SlashResult {
  const sessions = listSessions(db);

  if (argRest === "new" || argRest === "fresh") {
    return { newSession: true };
  }

  if (argRest) {
    const num = parseInt(argRest, 10);
    if (!isNaN(num) && num >= 1 && num <= sessions.length) {
      const picked = sessions[num - 1]!;
      const label = picked.label ?? picked.id.slice(0, 8);
      printInfo(`  Switching to session ${accent(label)}`);
      return { switchSession: { sessionId: picked.id } };
    }

    const byId = sessions.find((s) => s.id.startsWith(argRest));
    if (byId) {
      const label = byId.label ?? byId.id.slice(0, 8);
      printInfo(`  Switching to session ${accent(label)}`);
      return { switchSession: { sessionId: byId.id } };
    }

    const byLabel = sessions.find((s) => s.label?.toLowerCase() === argRest.toLowerCase());
    if (byLabel) {
      printInfo(`  Switching to session ${accent(byLabel.label!)}`);
      return { switchSession: { sessionId: byLabel.id } };
    }

    printError(`No session matching "${argRest}". Use /session to list.`);
    return;
  }

  if (sessions.length === 0) {
    printInfo("  No sessions yet.");
    return;
  }

  process.stdout.write("\n");
  for (let i = 0; i < sessions.length; i++) {
    const s = sessions[i]!;
    const label = s.label ?? muted("(unlabeled)");
    const active = s.id === db.activeSessionId ? accent(" ←") : "";
    const sep = muted("·");
    const info = `${String(s.turns)} turns ${sep} $${s.costUsd.toFixed(2)} ${sep} ${timeAgo(s.lastActiveAt)}`;
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${label}  ${info}${active}\n`);
  }
  process.stdout.write(`\n  ${muted("/session <#|id> to switch · /session new to start fresh")}\n\n`);
}

// ── /agent ───────────────────────────────────────────────────────────────

function handleAgentCommand(
  repoPath: string,
  _db: AgentDB,
  _argRest: string,
): SlashResult {
  const defs = listAgentDefs(repoPath);

  if (defs.length === 0) {
    printInfo("  No agent definitions found. Create .mp/agents/default.md to get started.");
    return;
  }

  process.stdout.write("\n");
  for (let i = 0; i < defs.length; i++) {
    const d = defs[i]!;
    const desc = d.description ? muted(d.description) : "";
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${d.name}  ${desc}\n`);
  }
  process.stdout.write(`\n  ${muted("Agent definitions in .mp/agents/")}\n\n`);
}
