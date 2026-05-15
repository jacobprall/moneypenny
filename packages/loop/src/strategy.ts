export interface StrategyMessage {
  role: "user" | "assistant" | "tool";
  content: string | null;
}

/** Alias for strategy hooks (matches common chat message shape). */
export type Message = StrategyMessage;

export interface IterationStrategy {
  preIteration(iteration: number, history: Message[]): StrategyAction;
  postIteration(iteration: number, response: string | null, history: Message[]): StrategyAction;
  finalize(): StrategyOutput | null;
}

export type StrategyAction =
  | { action: "continue" }
  | { action: "done" }
  | { action: "inject_user_message"; message: string };

export interface StrategyOutput {
  findings: string[];
  sources: string[];
  summary: string | null;
}

export class StandardStrategy implements IterationStrategy {
  preIteration(_iteration: number, _history: Message[]): StrategyAction {
    return { action: "continue" };
  }

  postIteration(_iteration: number, _response: string | null, _history: Message[]): StrategyAction {
    return { action: "done" };
  }

  finalize(): StrategyOutput | null {
    return null;
  }
}

const DEFAULT_RESEARCH_MAX_ITERATIONS = 5;

const RESEARCH_KICKOFF_PROMPT = `You are a focused researcher. Gather accurate information using tools as needed.

Output conventions (use these exact prefixes on their own lines when you have content):
- FINDING: <concise fact or insight>
- SOURCE: <URL, title, or citation for the finding above>

When your research pass is complete and you believe you have enough for the user's question, include a line containing exactly: RESEARCH_COMPLETE
You may optionally add SUMMARY: <brief synthesis> on its own line before or after RESEARCH_COMPLETE.

Continue searching until you can mark RESEARCH_COMPLETE or you truly cannot find more relevant material.`;

function researchProgressPrompt(findings: string[]): string {
  const listed =
    findings.length > 0
      ? findings.map((f, i) => `${i + 1}. ${f}`).join("\n")
      : "(none yet)";
  return `Research progress — findings so far:
${listed}

Identify gaps, contradictions, or missing angles. Use tools to search for more evidence. Add any new items using FINDING: and SOURCE: lines as before.

If this pass is sufficient, output RESEARCH_COMPLETE (and optionally SUMMARY:). If you cannot meaningfully extend the research, output RESEARCH_COMPLETE with a short SUMMARY: explaining what you covered and what remains unknown.`;
}

function parseMarkedLines(text: string, prefix: string): string[] {
  const out: string[] = [];
  const re = new RegExp(`^\\s*${prefix}:\\s*(.*)\\s*$`, "gim");
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const line = m[1]?.trim();
    if (line) out.push(line);
  }
  return out;
}

function hasResearchComplete(text: string): boolean {
  return /\bRESEARCH_COMPLETE\b/.test(text);
}

function parseSummary(text: string): string | null {
  const matches = parseMarkedLines(text, "SUMMARY");
  if (matches.length === 0) return null;
  return matches[matches.length - 1] ?? null;
}

export interface ResearchStrategyOptions {
  maxIterations?: number;
}

export class ResearchStrategy implements IterationStrategy {
  readonly maxIterations: number;
  private readonly findings: string[] = [];
  private readonly sources: string[] = [];
  private summary: string | null = null;
  private staleIterations = 0;

  constructor(options: ResearchStrategyOptions = {}) {
    this.maxIterations = options.maxIterations ?? DEFAULT_RESEARCH_MAX_ITERATIONS;
  }

  get findingsCount(): number {
    return this.findings.length;
  }

  preIteration(iteration: number, history: Message[]): StrategyAction {
    if (iteration >= this.maxIterations) {
      return { action: "done" };
    }

    if (iteration > 0) {
      const last = history[history.length - 1];
      if (last?.role === "tool") {
        return { action: "continue" };
      }
    }

    if (iteration === 0) {
      return { action: "inject_user_message", message: RESEARCH_KICKOFF_PROMPT };
    }
    return { action: "inject_user_message", message: researchProgressPrompt(this.findings) };
  }

  postIteration(iteration: number, response: string | null, _history: Message[]): StrategyAction {
    const text = response ?? "";

    if (hasResearchComplete(text)) {
      const s = parseSummary(text);
      if (s) this.summary = s;
      return { action: "done" };
    }

    const newFindings = parseMarkedLines(text, "FINDING");
    const newSources = parseMarkedLines(text, "SOURCE");

    let added = false;
    for (const f of newFindings) {
      if (!this.findings.includes(f)) {
        this.findings.push(f);
        added = true;
      }
    }
    for (const s of newSources) {
      if (!this.sources.includes(s)) {
        this.sources.push(s);
        added = true;
      }
    }

    const summaryLine = parseSummary(text);
    if (summaryLine) this.summary = summaryLine;

    if (added) {
      this.staleIterations = 0;
    } else {
      this.staleIterations += 1;
    }

    if (this.staleIterations >= 2) {
      return { action: "done" };
    }

    if (iteration >= this.maxIterations - 1) {
      return { action: "done" };
    }

    return { action: "continue" };
  }

  finalize(): StrategyOutput | null {
    return {
      findings: [...this.findings],
      sources: [...this.sources],
      summary: this.summary,
    };
  }
}
