import * as readline from "node:readline";
import {
  closeAgentDB,
  listSessions,
  type AgentBlueprint,
  type AgentDefInfo,
  type SessionSummary,
} from "@moneypenny/db";
import { bold, muted } from "./display.js";
import { getTheme } from "./theme.js";
import { listAgentDefs, openAgent } from "./session.js";
import type { WorkspaceDB } from "@moneypenny/db";
import { interactiveSelect } from "./select.js";

export function ask(rl: readline.Interface, prompt: string): Promise<string> {
  return new Promise((resolve) => rl.question(prompt, resolve));
}

export function timeAgo(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 60_000) return "just now";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
  return `${Math.floor(delta / 86_400_000)}d ago`;
}

export async function pickAgentDef(
  rl: readline.Interface,
  repoPath: string,
): Promise<AgentDefInfo> {
  const defs = listAgentDefs(repoPath);
  if (defs.length === 1) return defs[0]!;
  if (defs.length === 0) {
    return { name: "default", description: "General-purpose coding assistant", filePath: "" };
  }

  const options = defs.map((d) => ({
    label: d.name,
    value: d,
    hint: d.description,
  }));

  process.stdout.write(`\n  ${bold("Available agents:")}\n\n`);
  const picked = await interactiveSelect(options, { rl });
  return picked ?? defs[0]!;
}

export async function pickSession(
  rl: readline.Interface,
  sessions: SessionSummary[],
): Promise<{ action: "resume"; sessionId: string } | { action: "fresh" }> {
  if (sessions.length === 0) return { action: "fresh" };

  const t = getTheme();
  const sep = muted("·");

  type SessionChoice = { action: "resume"; sessionId: string } | { action: "fresh" };
  const options = sessions.map((s, i) => ({
    label: s.label ?? "unlabeled",
    value: { action: "resume" as const, sessionId: s.id } as SessionChoice,
    hint: [
      `${String(s.turns)} turn${s.turns === 1 ? "" : "s"}`,
      timeAgo(s.lastActiveAt),
      ...(i === 0 ? [t.sessionLatest] : []),
    ].join(` ${sep} `),
  }));
  options.push({
    label: t.sessionFreshLabel,
    value: { action: "fresh" as const } as SessionChoice,
  });

  process.stdout.write(`\n  ${bold(t.sessionHeader)}\n\n`);
  const picked = await interactiveSelect(options, { rl });
  return picked ?? { action: "resume", sessionId: sessions[0]!.id };
}

export interface ResolvedAgent {
  agentName: string;
  blueprint?: AgentBlueprint;
  startFreshSession: boolean;
  explicitSessionId?: string;
}

/**
 * Interactive agent/session picker flow. Uses agent definition files (.md)
 * to identify available agents. All agents share a single mp.db — session
 * selection scopes to the chosen agent_name.
 */
export async function resolveAgentInteractively(
  repoPath: string,
  workspace: WorkspaceDB,
  opts: { agent?: string; new?: boolean; session?: string },
): Promise<ResolvedAgent> {
  if (opts.agent || opts.new) {
    return {
      agentName: opts.agent ?? "default",
      startFreshSession: Boolean(opts.new),
      explicitSessionId: opts.session,
    };
  }

  const defs = listAgentDefs(repoPath);
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

  try {
    let agentName = "default";
    let startFreshSession = false;
    let explicitSessionId = opts.session;

    if (defs.length > 1) {
      const picked = await pickAgentDef(rl, repoPath);
      agentName = picked.name;
    } else if (defs.length === 1) {
      agentName = defs[0]!.name;
    }

    if (!startFreshSession && !explicitSessionId) {
      const tempDb = openAgent(repoPath, { name: agentName, workspace });
      const sessions = listSessions(tempDb, { agentName });
      closeAgentDB(tempDb);

      if (sessions.length > 0) {
        const sessionChoice = await pickSession(rl, sessions);
        if (sessionChoice.action === "fresh") {
          startFreshSession = true;
        } else {
          explicitSessionId = sessionChoice.sessionId;
        }
      } else {
        startFreshSession = true;
      }
    }

    return { agentName, startFreshSession, explicitSessionId };
  } finally {
    rl.close();
  }
}
