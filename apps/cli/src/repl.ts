import type { Database } from "bun:sqlite";
import { getHealth } from "@moneypenny/db";
import { assembleSystemPrompt } from "@moneypenny/db";

const BOLD = "\x1b[1m";
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";
const GREEN = "\x1b[32m";
const CYAN = "\x1b[36m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const MAGENTA = "\x1b[35m";

export function formatMarkdown(text: string): string {
  let result = text;

  result = result.replace(/```(\w*)\n([\s\S]*?)```/g, (_, lang, code) => {
    const header = lang ? `${DIM}─── ${lang} ───${RESET}\n` : "";
    return `${header}${CYAN}${code.trimEnd()}${RESET}`;
  });

  result = result.replace(/`([^`]+)`/g, `${CYAN}$1${RESET}`);

  result = result.replace(/^### (.+)$/gm, `${BOLD}${YELLOW}   $1${RESET}`);
  result = result.replace(/^## (.+)$/gm, `${BOLD}${GREEN}  $1${RESET}`);
  result = result.replace(/^# (.+)$/gm, `${BOLD}${MAGENTA} $1${RESET}`);

  result = result.replace(/\*\*([^*]+)\*\*/g, `${BOLD}$1${RESET}`);
  result = result.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, `${DIM}$1${RESET}`);

  result = result.replace(/^(\s*)- /gm, `$1${DIM}•${RESET} `);
  result = result.replace(/^(\s*)\d+\. /gm, (match, indent) => {
    return `${indent}${DIM}${match.trim()}${RESET} `;
  });

  return result;
}

export interface SlashCommandResult {
  handled: boolean;
  output?: string;
  action?: "clear" | "quit";
}

export function handleSlashCommand(
  input: string,
  db: Database,
  sessionId: string,
  agentName: string,
): SlashCommandResult {
  const parts = input.slice(1).split(/\s+/);
  const cmd = parts[0];
  const args = parts.slice(1);

  switch (cmd) {
    case "help":
      return {
        handled: true,
        output: `${BOLD}Slash commands:${RESET}
  ${CYAN}/help${RESET}           Show this help
  ${CYAN}/context${RESET}        Show current system prompt
  ${CYAN}/cost${RESET}           Show session + daily cost
  ${CYAN}/sessions${RESET}       List recent sessions
  ${CYAN}/status${RESET}         Show system health
  ${CYAN}/skills${RESET}         List learned skills
  ${CYAN}/conventions${RESET}    List project conventions
  ${CYAN}/clear${RESET}          Clear conversation history
  ${CYAN}/quit${RESET}           Exit chat`,
      };

    case "context": {
      const prompt = assembleSystemPrompt(db, agentName);
      return { handled: true, output: `${DIM}${prompt}${RESET}` };
    }

    case "cost": {
      const session = db
        .query<{ cost: number; tokens_in: number; tokens_out: number }, [string]>(
          `SELECT COALESCE(SUM(cost_usd), 0) as cost,
                  COALESCE(SUM(tokens_in), 0) as tokens_in,
                  COALESCE(SUM(tokens_out), 0) as tokens_out
           FROM messages WHERE session_id = ?`,
        )
        .get(sessionId);

      const daily = db
        .query<{ total: number; sessions: number }, []>(
          "SELECT COALESCE(total, 0) as total, COALESCE(sessions, 0) as sessions FROM v_cost_today",
        )
        .get();

      const budgetRow = db
        .query<{ conditions: string | null }, [string]>(
          "SELECT conditions FROM policies WHERE name = ?",
        )
        .get("Budget Guard");

      let budgetInfo = "";
      if (budgetRow?.conditions) {
        const c = JSON.parse(budgetRow.conditions);
        budgetInfo = `\n  ${DIM}budget: $${c.maxSessionUsd}/session, $${c.maxDailyUsd}/day${RESET}`;
      }

      return {
        handled: true,
        output: `  ${BOLD}session:${RESET} $${(session?.cost ?? 0).toFixed(4)}  (${session?.tokens_in ?? 0} in / ${session?.tokens_out ?? 0} out)
  ${BOLD}today:${RESET}   $${(daily?.total ?? 0).toFixed(4)}  (${daily?.sessions ?? 0} sessions)${budgetInfo}`,
      };
    }

    case "sessions": {
      const sessions = db
        .query<
          { id: string; label: string | null; agent_name: string | null; created_at: number },
          []
        >(
          "SELECT id, label, agent_name, created_at FROM sessions ORDER BY created_at DESC LIMIT 10",
        )
        .all();

      if (sessions.length === 0) return { handled: true, output: "  No sessions yet." };

      const lines = sessions.map((s) => {
        const date = new Date(s.created_at * 1000).toISOString().split("T")[0];
        return `  ${DIM}${s.id.slice(0, 8)}${RESET}  ${date}  ${s.label ?? "(unlabeled)"}`;
      });
      return { handled: true, output: lines.join("\n") };
    }

    case "status": {
      const health = getHealth(db);
      return { handled: true, output: JSON.stringify(health, null, 2) };
    }

    case "skills": {
      const skills = db
        .query<{ name: string; description: string; confidence: number }, []>(
          "SELECT name, description, confidence FROM skills WHERE confidence > 0.3 ORDER BY confidence DESC",
        )
        .all();

      if (skills.length === 0) return { handled: true, output: "  No skills learned yet." };

      const lines = skills.map(
        (s) =>
          `  ${BOLD}${s.name}${RESET} ${DIM}(${(s.confidence * 100).toFixed(0)}%)${RESET}: ${s.description}`,
      );
      return { handled: true, output: lines.join("\n") };
    }

    case "conventions": {
      const convs = db
        .query<{ name: string; category: string; description: string; confidence: number }, []>(
          "SELECT name, category, description, confidence FROM conventions WHERE confidence > 0.3 ORDER BY confidence DESC",
        )
        .all();

      if (convs.length === 0)
        return { handled: true, output: "  No conventions detected." };

      const lines = convs.map(
        (c) =>
          `  ${DIM}[${c.category}]${RESET} ${BOLD}${c.name}${RESET}: ${c.description}`,
      );
      return { handled: true, output: lines.join("\n") };
    }

    case "clear":
      return { handled: true, action: "clear" };

    case "quit":
    case "exit":
      return { handled: true, action: "quit" };

    default:
      return { handled: true, output: `  Unknown command: /${cmd}. Type /help for commands.` };
  }
}

export function formatToolProgress(toolName: string, phase: "start" | "end", resultSummary?: string): string {
  if (phase === "start") {
    return `${DIM}  ⟶ calling ${toolName}...${RESET}`;
  }
  const summary = resultSummary ? `: ${resultSummary}` : "";
  return `${DIM}  ⟵ ${toolName}${summary}${RESET}`;
}
