import type { Database } from "bun:sqlite";
import { assembleSystemPrompt } from "@moneypenny/db";

export type ContextView = "default" | "coding" | "research" | "refactoring" | "review";

interface ViewConfig {
  sections: string[];
  suffix: string;
}

const VIEWS: Record<ContextView, ViewConfig> = {
  default: {
    sections: ["identity", "sessions", "skills", "conventions", "policies"],
    suffix: "",
  },
  coding: {
    sections: ["identity", "conventions", "policies", "skills"],
    suffix: `

## Coding Context
You are in coding mode. You have tools to read, write, and search files, and to run commands.
When making changes:
1. Read the relevant files first to understand context
2. Make targeted, minimal changes
3. Run tests or builds to verify your changes work
4. Follow project conventions detected from the codebase
`,
  },
  research: {
    sections: ["identity", "skills", "sessions"],
    suffix: `

## Research Context
You are in research mode. Focus on thorough investigation:
1. Search code and messages broadly before narrowing
2. For each finding, note the SOURCE (file, function, session)
3. Identify GAPS in understanding
4. When done, provide a structured RESEARCH_COMPLETE report
`,
  },
  refactoring: {
    sections: ["identity", "conventions", "policies"],
    suffix: `

## Refactoring Context
You are in refactoring mode. Focus on structural improvements:
1. Understand the current architecture before changing it
2. Preserve all existing behavior (no functional changes)
3. Follow project conventions strictly
4. Make small, incremental changes that can be verified
`,
  },
  review: {
    sections: ["identity", "conventions", "policies"],
    suffix: `

## Review Context
You are in code review mode. Focus on:
1. Correctness — does the code do what it claims?
2. Conventions — does it follow project patterns?
3. Edge cases — what inputs could break it?
4. Maintainability — will this be clear in 6 months?
Be specific and actionable in feedback.
`,
  },
};

export function assembleContextForView(
  db: Database,
  agentName: string,
  view: ContextView,
  customInstructions?: string,
): string {
  const config = VIEWS[view];
  const base = assembleSystemPrompt(db, agentName, customInstructions);

  const parts = [base];

  if (config.sections.includes("conventions") && view !== "default") {
    const convs = db
      .query<{ name: string; description: string }, []>(
        "SELECT name, description FROM conventions WHERE confidence > 0.5 ORDER BY confidence DESC LIMIT 15",
      )
      .all();
    if (convs.length > 0) {
      const formatted = convs.map((c) => `- ${c.name}: ${c.description}`).join("\n");
      if (!base.includes("## Project Conventions")) {
        parts.push(`## Project Conventions\n${formatted}`);
      }
    }
  }

  if (view === "coding") {
    const langs = db
      .query<{ language: string; cnt: number }, []>(
        "SELECT language, COUNT(*) as cnt FROM code_chunks WHERE language IS NOT NULL GROUP BY language ORDER BY cnt DESC LIMIT 5",
      )
      .all();
    if (langs.length > 0) {
      parts.push(`## Codebase Profile\nLanguages: ${langs.map((l) => `${l.language} (${l.cnt} chunks)`).join(", ")}`);
    }
  }

  if (config.suffix) {
    parts.push(config.suffix);
  }

  return parts.join("\n\n");
}
