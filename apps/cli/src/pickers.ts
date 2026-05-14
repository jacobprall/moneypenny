import * as readline from "node:readline";
import {
  closeAgentDB,
  discoverBlueprints,
  listSessions,
  type AgentBlueprint,
  type SessionSummary,
} from "@swe/db";
import { bold, muted } from "./display.js";
import { listAgents, openAgent, type AgentInfo } from "./session.js";
import type { WorkspaceDB } from "@swe/db";

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

export async function pickBlueprint(
  rl: readline.Interface,
  repoPath: string,
): Promise<AgentBlueprint> {
  const blueprints = discoverBlueprints(repoPath);
  if (blueprints.length === 1) return blueprints[0]!;

  process.stdout.write(`\n  ${bold("Available blueprints:")}\n\n`);
  for (let i = 0; i < blueprints.length; i++) {
    const bp = blueprints[i]!;
    const desc = bp.description ? muted(bp.description) : "";
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${bp.name} ${desc}\n`);
  }

  const ans = await ask(rl, `\n  Select blueprint ${muted(`[1]`)}: `);
  const idx = parseInt(ans.trim(), 10);
  if (idx >= 1 && idx <= blueprints.length) return blueprints[idx - 1]!;
  return blueprints[0]!;
}

export async function pickAgent(
  rl: readline.Interface,
  agents: AgentInfo[],
): Promise<{ action: "existing"; agent: AgentInfo } | { action: "new" }> {
  process.stdout.write(`\n  ${bold("Agents in this repo:")}\n\n`);
  for (let i = 0; i < agents.length; i++) {
    const a = agents[i]!;
    const bpLabel = a.blueprintDescription ?? a.blueprintName ?? "unknown";
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${a.name}  ${muted(bpLabel)}\n`);
  }
  process.stdout.write(`    ${muted("n.")} Create new agent\n`);

  const ans = await ask(rl, `\n  Select agent ${muted(`[1]`)}: `);
  const trimmed = ans.trim().toLowerCase();

  if (trimmed === "n" || trimmed === "new") return { action: "new" };

  const idx = parseInt(trimmed, 10);
  if (idx >= 1 && idx <= agents.length) return { action: "existing", agent: agents[idx - 1]! };
  return { action: "existing", agent: agents[0]! };
}

export async function pickSession(
  rl: readline.Interface,
  sessions: SessionSummary[],
): Promise<{ action: "resume"; sessionId: string } | { action: "fresh" }> {
  if (sessions.length === 0) return { action: "fresh" };

  process.stdout.write(`\n  ${bold("Sessions:")}\n\n`);
  for (let i = 0; i < sessions.length; i++) {
    const s = sessions[i]!;
    const label = s.label ?? muted("(unlabeled)");
    const info = `${String(s.turns)} turns ${muted("·")} $${s.costUsd.toFixed(2)} ${muted("·")} ${timeAgo(s.lastActiveAt)}`;
    process.stdout.write(`    ${muted(String(i + 1) + ".")} ${label}  ${info}\n`);
  }

  const ans = await ask(rl, `\n  ${muted("[r]esume latest, [f]resh session, or [#] to pick?")} ${muted("[r]")}: `);
  const trimmed = ans.trim().toLowerCase();

  if (trimmed === "f" || trimmed === "fresh") return { action: "fresh" };

  const idx = parseInt(trimmed, 10);
  if (idx >= 1 && idx <= sessions.length) return { action: "resume", sessionId: sessions[idx - 1]!.id };

  return { action: "resume", sessionId: sessions[0]!.id };
}

export interface ResolvedAgent {
  agentName: string;
  blueprint?: AgentBlueprint;
  startFreshSession: boolean;
  explicitSessionId?: string;
}

/**
 * Interactive agent/session picker flow. If only one agent with a default name
 * exists, skips straight to session selection. Returns enough info for the
 * caller to open the correct DB and session.
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

  const agents = listAgents(repoPath);
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

  try {
    let agentName = "default";
    let blueprint: AgentBlueprint | undefined;
    let startFreshSession = false;
    let explicitSessionId = opts.session;

    if (agents.length === 0) {
      blueprint = await pickBlueprint(rl, repoPath);
      agentName = blueprint.name === "swe-default" ? "default" : blueprint.name;
      startFreshSession = true;
    } else if (agents.length === 1 && agents[0]!.name === "default") {
      agentName = "default";
    } else {
      const agentChoice = await pickAgent(rl, agents);
      if (agentChoice.action === "new") {
        blueprint = await pickBlueprint(rl, repoPath);
        agentName = blueprint.name === "swe-default" ? "default" : blueprint.name;
        startFreshSession = true;
      } else {
        agentName = agentChoice.agent.name;
      }
    }

    if (!startFreshSession && !explicitSessionId) {
      const tempDb = openAgent(repoPath, { name: agentName, workspace });
      const sessions = listSessions(tempDb);
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

    return { agentName, blueprint, startFreshSession, explicitSessionId };
  } finally {
    rl.close();
  }
}
