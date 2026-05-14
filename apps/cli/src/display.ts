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

// ── Pip-Boy palette ─────────────────────────────────────────────────────

export const accent = rgb(57, 255, 20, "32");
export const data = rgb(0, 229, 255, "36");
export const success = rgb(57, 255, 20, "32");
export const error = rgb(255, 49, 49, "31");
export const warning = rgb(255, 182, 39, "33");
export const chrome = rgb(90, 110, 90, "2");

export const muted = chrome;

// ── Box drawing ─────────────────────────────────────────────────────────

const W = 40;

function hline(left: string, fill: string, right: string, width = W): string {
  return muted(`${left}${fill.repeat(width)}${right}`);
}

function row(content: string, width = W): string {
  const stripped = content.replace(/\x1b\[[0-9;]*m/g, "");
  const pad = Math.max(0, width - stripped.length - 1);
  return `${muted("║")}  ${content}${" ".repeat(pad)}${muted("║")}`;
}

// ── Spinner ──────────────────────────────────────────────────────────────

const SCAN_FRAMES = [
  "[■□□□]",
  "[□■□□]",
  "[□□■□]",
  "[□□□■]",
  "[□□■□]",
  "[□■□□]",
];

export class Spinner {
  private interval: ReturnType<typeof setInterval> | null = null;
  private frameIdx = 0;

  start(text: string): void {
    this.frameIdx = 0;
    this.render(text);
    this.interval = setInterval(() => {
      this.frameIdx++;
      this.render(text);
    }, 120);
  }

  stop(): void {
    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
      process.stdout.write("\r\x1b[2K");
    }
  }

  private render(text: string): void {
    const frame = SCAN_FRAMES[this.frameIdx % SCAN_FRAMES.length]!;
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

export function printToolStart(name: string, input: unknown): void {
  const summary =
    typeof input === "object" && input !== null
      ? JSON.stringify(input).slice(0, 60)
      : String(input);
  const label = accent(name);
  const rest = chrome(summary);
  process.stdout.write(`  ${chrome("┌─[")} ${label} ${chrome("]")}${chrome("─".repeat(Math.max(1, 32 - name.length)))}\n`);
  if (summary) {
    process.stdout.write(`  ${chrome("│")}  ${rest}\n`);
  }
}

export function printToolComplete(_name: string, output: string, durationMs: number): void {
  const isFileOp = /^file_(edit|write|patch)$/.test(_name);
  const lines = output.split("\n");

  if (isFileOp && output.includes("@@")) {
    printDiffLines(lines);
  } else {
    const maxShow = 5;
    for (const line of lines.slice(0, maxShow)) {
      process.stdout.write(`  ${chrome("│")}  ${line}\n`);
    }
    if (lines.length > maxShow) {
      process.stdout.write(
        `  ${chrome("│")}  ${chrome(`... ${String(lines.length - maxShow)} more lines`)}\n`,
      );
    }
  }

  process.stdout.write(`  ${chrome("└─")} ${data(formatDuration(durationMs))} ${chrome("─".repeat(30))}\n\n`);
}

function printDiffLines(lines: string[]): void {
  for (const line of lines) {
    if (line.startsWith("+")) {
      process.stdout.write(`  ${chrome("│")}  ${success(line)}\n`);
    } else if (line.startsWith("-")) {
      process.stdout.write(`  ${chrome("│")}  ${error(line)}\n`);
    } else if (line.startsWith("@@")) {
      process.stdout.write(`  ${chrome("│")}  ${chrome(line)}\n`);
    } else {
      process.stdout.write(`  ${chrome("│")}  ${line}\n`);
    }
  }
}

export function printToolError(_name: string, err: string): void {
  process.stdout.write(`  ${chrome("└─")} ${error("[ERR]")} ${err}\n\n`);
}

// ── Cost / turn footer ──────────────────────────────────────────────────

export function printCost(cost: {
  model: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
  turnNumber: number;
}): void {
  const turnLabel = chrome(`TURN ${String(cost.turnNumber)}`);
  const inp = `${data("IN")} ${humanTokens(cost.inputTokens)}`;
  const out = `${data("OUT")} ${humanTokens(cost.outputTokens)}`;
  const usd = `${accent("$" + cost.costUsd.toFixed(4))}`;
  const model = chrome(cost.model);

  process.stdout.write(`\n  ${chrome("[")} ${turnLabel} ${chrome("]")}  ${inp}  ${out}  ${usd}  ${model}\n`);
}

// ── Turn separator ──────────────────────────────────────────────────────

export function printTurnSeparator(): void {
  process.stdout.write(`\n  ${chrome("░▒▓")} ${chrome("─".repeat(32))} ${chrome("▓▒░")}\n`);
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

  process.stdout.write("\n");
  process.stdout.write(`  ${hline("╔═", "═", "═╗")}\n`);
  process.stdout.write(`  ${row(`${bold(accent("swe"))} ${chrome(`v${opts.version}`)}`)}\n`);
  process.stdout.write(`  ${hline("╠─", "─", "─╣")}\n`);
  process.stdout.write(`  ${row(`${chrome("session")}  ${accent(opts.session)}`)}\n`);
  process.stdout.write(`  ${row(`${chrome("model")}    ${accent(modelDisplay)}`)}\n`);
  process.stdout.write(`  ${row(`${chrome("repo")}     ${accent(repo)}`)}\n`);
  process.stdout.write(`  ${hline("╠─", "─", "─╣")}\n`);
  process.stdout.write(`  ${row(`${accent("/help")} ${chrome("commands")}  ${accent("/exit")} ${chrome("quit")}`)}\n`);
  process.stdout.write(`  ${hline("╚═", "═", "═╝")}\n`);
  process.stdout.write("\n");
}

// ── Help ─────────────────────────────────────────────────────────────────

export function printHelp(): void {
  const cmd = (name: string, args: string, desc: string): string => {
    const left = `${accent(name)} ${chrome(args)}`;
    const rawLen = name.length + 1 + args.length;
    const padding = Math.max(1, 22 - rawLen);
    return `  ${chrome("║")}  ${left}${" ".repeat(padding)}${chrome(desc)}`;
  };

  process.stdout.write("\n");
  process.stdout.write(`  ${hline("╔═", "═", "═╗")}\n`);
  process.stdout.write(`  ${row(bold(accent("COMMANDS")))}\n`);
  process.stdout.write(`  ${hline("╠─", "─", "─╣")}\n`);
  process.stdout.write(cmd("/compact", "[msg]", "Compact conversation history") + "\n");
  process.stdout.write(cmd("/fresh", "     ", "Start a fresh session") + "\n");
  process.stdout.write(cmd("/sessions", "  ", "List sessions") + "\n");
  process.stdout.write(cmd("/agents", "    ", "List agents in this repo") + "\n");
  process.stdout.write(cmd("/search", " <q>", "Search the codebase") + "\n");
  process.stdout.write(cmd("/index", "    ", "Rebuild the code index") + "\n");
  process.stdout.write(cmd("/model", " [id]", "List or switch models") + "\n");
  process.stdout.write(`  ${hline("╠─", "─", "─╣")}\n`);
  process.stdout.write(cmd("/cost", "     ", "Session cost & tokens") + "\n");
  process.stdout.write(cmd("/status", "   ", "Index and session status") + "\n");
  process.stdout.write(cmd("/help", "     ", "Show this help") + "\n");
  process.stdout.write(cmd("/exit", "     ", "End session") + "\n");
  process.stdout.write(`  ${hline("╚═", "═", "═╝")}\n`);
  process.stdout.write("\n");
}
