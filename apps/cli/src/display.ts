export const COLORS_ENABLED = !process.env.SWE_NO_COLOR && !process.env.NO_COLOR;

const TRUECOLOR =
  COLORS_ENABLED &&
  (process.env.COLORTERM === "truecolor" || process.env.COLORTERM === "24bit");

function ansi(code: string): (s: string) => string {
  return (s) => (COLORS_ENABLED ? `\x1b[${code}m${s}\x1b[0m` : s);
}

function rgb(r: number, g: number, b: number, fallbackCode: string): (s: string) => string {
  if (!COLORS_ENABLED) return (s) => s;
  if (TRUECOLOR) return (s) => `\x1b[38;2;${r};${g};${b}m${s}\x1b[0m`;
  return ansi(fallbackCode);
}

// ── Base formatters ──────────────────────────────────────────────────────

export const dim = ansi("2");
export const bold = ansi("1");
export const italic = ansi("3");

// ── Palette ─────────────────────────────────────────────────────────────

export const accent = rgb(94, 214, 148, "32");
export const data = rgb(130, 190, 230, "36");
export const success = rgb(72, 199, 142, "32");
export const error = rgb(235, 87, 87, "31");
export const warning = rgb(240, 185, 80, "33");
export const chrome = rgb(108, 118, 128, "2");

export const muted = chrome;

// ── Layout helpers ──────────────────────────────────────────────────────

function rule(width = 40): string {
  return chrome("─".repeat(width));
}

// ── Spinner ──────────────────────────────────────────────────────────────

const SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

export class Spinner {
  private interval: ReturnType<typeof setInterval> | null = null;
  private frameIdx = 0;

  start(text: string): void {
    this.frameIdx = 0;
    this.render(text);
    this.interval = setInterval(() => {
      this.frameIdx++;
      this.render(text);
    }, 80);
  }

  stop(): void {
    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
      process.stdout.write("\r\x1b[2K");
    }
  }

  private render(text: string): void {
    const frame = SPINNER_FRAMES[this.frameIdx % SPINNER_FRAMES.length]!;
    process.stdout.write(`\r\x1b[2K  ${accent(frame)} ${chrome(text)}`);
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────

export function shortenPath(p: string): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

function humanTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${String(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

// ── Tool output ──────────────────────────────────────────────────────────

function toolSummary(input: unknown): string {
  if (typeof input !== "object" || input === null) return String(input);
  const obj = input as Record<string, unknown>;
  const path = obj.path ?? obj.file ?? obj.command ?? obj.query;
  if (typeof path === "string") return path.length > 60 ? path.slice(0, 57) + "..." : path;
  return JSON.stringify(input).slice(0, 60);
}

export function printToolStart(name: string, input: unknown): void {
  const summary = toolSummary(input);
  process.stdout.write(`  ${accent("▸")} ${accent(name)}  ${chrome(summary)}\n`);
}

export function printToolComplete(_name: string, output: string, durationMs: number): void {
  const isFileOp = /^file_(edit|write|patch)$/.test(_name);
  const lines = output.split("\n");

  if (isFileOp && output.includes("@@")) {
    printDiffLines(lines);
  } else {
    const maxShow = 5;
    for (const line of lines.slice(0, maxShow)) {
      process.stdout.write(`    ${chrome("│")} ${line}\n`);
    }
    if (lines.length > maxShow) {
      process.stdout.write(
        `    ${chrome("│")} ${chrome(`… ${String(lines.length - maxShow)} more lines`)}\n`,
      );
    }
  }

  process.stdout.write(`    ${chrome("╰")} ${data(formatDuration(durationMs))}\n\n`);
}

function printDiffLines(lines: string[]): void {
  for (const line of lines) {
    if (line.startsWith("+")) {
      process.stdout.write(`    ${chrome("│")} ${success(line)}\n`);
    } else if (line.startsWith("-")) {
      process.stdout.write(`    ${chrome("│")} ${error(line)}\n`);
    } else if (line.startsWith("@@")) {
      process.stdout.write(`    ${chrome("│")} ${chrome(line)}\n`);
    } else {
      process.stdout.write(`    ${chrome("│")} ${line}\n`);
    }
  }
}

export function printToolError(_name: string, err: string): void {
  process.stdout.write(`    ${chrome("╰")} ${error("error")} ${err}\n\n`);
}

// ── Cost / turn footer ──────────────────────────────────────────────────

export function printCost(cost: {
  model: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
  turnNumber: number;
}): void {
  const sep = chrome("·");
  const parts = [
    chrome(`turn ${String(cost.turnNumber)}`),
    `${humanTokens(cost.inputTokens)} in`,
    `${humanTokens(cost.outputTokens)} out`,
    `$${cost.costUsd.toFixed(4)}`,
    chrome(cost.model),
  ];
  process.stdout.write(`\n  ${chrome(parts.join(` ${sep} `))}\n`);
}

// ── Turn separator ──────────────────────────────────────────────────────

export function printTurnSeparator(): void {
  process.stdout.write(`\n  ${rule(44)}\n`);
}

// ── Standard messages ────────────────────────────────────────────────────

export function printError(msg: string): void {
  process.stderr.write(`  ${error("[ERR]")} ${msg}\n`);
}

export function printInfo(msg: string): void {
  process.stdout.write(`${chrome(msg)}\n`);
}

export function printWarn(msg: string): void {
  process.stderr.write(`  ${warning("[!!]")} ${msg}\n`);
}

export function printDebug(msg: string): void {
  if (process.env.SWE_VERBOSE === "1" || process.env.DEBUG) {
    process.stderr.write(`  ${chrome("[dbg]")} ${chrome(msg)}\n`);
  }
}

// ── Banner ──────────────────────────────────────────────────────────────

export function printBanner(opts: {
  version: string;
  session: string;
  model: string;
  provider?: string;
  repoPath: string;
}): void {
  const repo = shortenPath(opts.repoPath);

  const modelDisplay = opts.provider && opts.provider !== "anthropic"
    ? `${opts.model} ${chrome(`(${opts.provider})`)}`
    : opts.model;

  const sep = chrome("·");

  process.stdout.write("\n");
  process.stdout.write(`  ${bold(accent("swe"))} ${chrome(`v${opts.version}`)}\n`);
  process.stdout.write(`  ${modelDisplay} ${sep} ${repo} ${sep} ${chrome(opts.session)}\n`);
  process.stdout.write(`  ${chrome("/help for commands")} ${sep} ${chrome("/exit to quit")}\n`);
  process.stdout.write("\n");
}

// ── Help ─────────────────────────────────────────────────────────────────

export function printHelp(): void {
  const cmd = (name: string, args: string, desc: string): string => {
    const left = `${accent(name)}${args ? " " + chrome(args) : ""}`;
    const rawLen = name.length + (args ? 1 + args.length : 0);
    const padding = Math.max(2, 20 - rawLen);
    return `    ${left}${" ".repeat(padding)}${chrome(desc)}`;
  };

  process.stdout.write("\n");
  process.stdout.write(`  ${bold("Commands")}\n\n`);
  process.stdout.write(cmd("/compact", "[msg]", "Compact conversation history") + "\n");
  process.stdout.write(cmd("/fresh", "", "Start a fresh session") + "\n");
  process.stdout.write(cmd("/sessions", "", "List sessions") + "\n");
  process.stdout.write(cmd("/agents", "", "List agents in this repo") + "\n");
  process.stdout.write(cmd("/search", "<q>", "Search the codebase") + "\n");
  process.stdout.write(cmd("/index", "", "Rebuild the code index") + "\n");
  process.stdout.write(cmd("/model", "[id]", "List or switch models") + "\n");
  process.stdout.write("\n");
  process.stdout.write(cmd("/cost", "", "Session cost & tokens") + "\n");
  process.stdout.write(cmd("/status", "", "Index and session status") + "\n");
  process.stdout.write(cmd("/help", "", "Show this help") + "\n");
  process.stdout.write(cmd("/exit", "", "End session") + "\n");
  process.stdout.write("\n");
}
