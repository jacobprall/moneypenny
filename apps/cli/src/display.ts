import { getTheme, type ThemeColors } from "./theme.js";

export const COLORS_ENABLED = !process.env.MP_NO_COLOR && !process.env.NO_COLOR;

const TRUECOLOR =
  COLORS_ENABLED &&
  (process.env.COLORTERM === "truecolor" || process.env.COLORTERM === "24bit");

// ── ANSI helpers ────────────────────────────────────────────────────────

function ansi(code: string): (s: string) => string {
  return (s) => (COLORS_ENABLED ? `\x1b[${code}m${s}\x1b[0m` : s);
}

function makeColor(key: keyof ThemeColors): (s: string) => string {
  return (s: string): string => {
    if (!COLORS_ENABLED) return s;
    const [r, g, b, fb] = getTheme().colors[key];
    if (TRUECOLOR) return `\x1b[38;2;${r};${g};${b}m${s}\x1b[0m`;
    return `\x1b[${fb}m${s}\x1b[0m`;
  };
}

// ── Base formatters (theme-independent) ─────────────────────────────────

export const dim = ansi("2");
export const bold = ansi("1");
export const italic = ansi("3");

// ── Palette (reads from active theme) ───────────────────────────────────

export const accent = makeColor("accent");
export const data = makeColor("data");
export const success = makeColor("success");
export const error = makeColor("error");
export const warning = makeColor("warning");
export const chrome = makeColor("chrome");

export const muted = chrome;

// ── Spinner ─────────────────────────────────────────────────────────────

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
    const t = getTheme();
    const frame = t.spinner[this.frameIdx % t.spinner.length]!;
    process.stdout.write(`\r\x1b[2K  ${accent(frame)} ${chrome(text)}`);
  }
}

// ── Helpers ─────────────────────────────────────────────────────────────

export function shortenPath(p: string): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
  if (home && p.startsWith(home)) return "~" + p.slice(home.length);
  return p;
}

export function humanTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${String(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

// ── Tool output ─────────────────────────────────────────────────────────

function toolSummary(input: unknown): string {
  if (typeof input !== "object" || input === null) return String(input);
  const obj = input as Record<string, unknown>;
  const path = obj.path ?? obj.file ?? obj.command ?? obj.query;
  if (typeof path === "string") return path.length > 60 ? path.slice(0, 57) + "..." : path;
  return JSON.stringify(input).slice(0, 60);
}

export function printToolStart(name: string, input: unknown): void {
  const t = getTheme();
  const summary = toolSummary(input);
  process.stdout.write(`  ${accent(t.toolMarker)} ${accent(name)}  ${chrome(summary)}\n`);
}

export function printToolComplete(_name: string, output: string, durationMs: number): void {
  const t = getTheme();
  const isFileOp = /^file_(edit|write|patch)$/.test(_name);
  const lines = output.split("\n");

  if (isFileOp && output.includes("@@")) {
    printDiffLines(lines);
  } else {
    const maxShow = 5;
    for (const line of lines.slice(0, maxShow)) {
      process.stdout.write(`    ${chrome(t.pipe)} ${line}\n`);
    }
    if (lines.length > maxShow) {
      process.stdout.write(
        `    ${chrome(t.pipe)} ${chrome(`… ${String(lines.length - maxShow)} more lines`)}\n`,
      );
    }
  }

  process.stdout.write(`    ${chrome(t.corner)} ${data(formatDuration(durationMs))}\n\n`);
}

function printDiffLines(lines: string[]): void {
  const t = getTheme();
  for (const line of lines) {
    if (line.startsWith("+")) {
      process.stdout.write(`    ${chrome(t.pipe)} ${success(line)}\n`);
    } else if (line.startsWith("-")) {
      process.stdout.write(`    ${chrome(t.pipe)} ${error(line)}\n`);
    } else if (line.startsWith("@@")) {
      process.stdout.write(`    ${chrome(t.pipe)} ${chrome(line)}\n`);
    } else {
      process.stdout.write(`    ${chrome(t.pipe)} ${line}\n`);
    }
  }
}

export function printToolError(_name: string, err: string): void {
  const t = getTheme();
  process.stdout.write(`    ${chrome(t.corner)} ${error("error")} ${err}\n\n`);
}

// ── Cost / turn footer ─────────────────────────────────────────────────

export function printCost(cost: {
  model: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
  turnNumber: number;
}): void {
  const t = getTheme();
  const sep = chrome("·");

  const tokenPart = t.costTokenFormat === "compact"
    ? `${humanTokens(cost.inputTokens)}${chrome("→")}${humanTokens(cost.outputTokens)}`
    : `${humanTokens(cost.inputTokens)} in ${sep} ${humanTokens(cost.outputTokens)} out`;

  const parts = [
    chrome(`${t.turnLabel} ${String(cost.turnNumber)}`),
    tokenPart,
    `$${cost.costUsd.toFixed(4)}`,
    chrome(cost.model),
  ];
  process.stdout.write(`\n  ${chrome(parts.join(` ${sep} `))}\n`);
}

// ── Turn separator ─────────────────────────────────────────────────────

export function printTurnSeparator(): void {
  const t = getTheme();
  const width = Math.ceil(44 / t.separator.length);
  process.stdout.write(`\n  ${chrome(t.separator.repeat(width).slice(0, 44))}\n`);
}

// ── Standard messages ──────────────────────────────────────────────────

export function printError(msg: string): void {
  const t = getTheme();
  process.stderr.write(`  ${error(t.errorTag)} ${msg}\n`);
}

export function printInfo(msg: string): void {
  process.stdout.write(`${chrome(msg)}\n`);
}

export function printWarn(msg: string): void {
  const t = getTheme();
  process.stderr.write(`  ${warning(t.warnTag)} ${msg}\n`);
}

export function printDebug(msg: string): void {
  if (process.env.MP_VERBOSE === "1" || process.env.DEBUG) {
    process.stderr.write(`  ${chrome("[dbg]")} ${chrome(msg)}\n`);
  }
}

// ── Banner ─────────────────────────────────────────────────────────────

export function printBanner(opts: {
  version: string;
  session: string;
  model: string;
  provider?: string;
  repoPath: string;
}): void {
  const t = getTheme();
  const repo = shortenPath(opts.repoPath);
  const sep = chrome("·");

  const modelDisplay = opts.provider && opts.provider !== "anthropic"
    ? `${opts.model} ${chrome(`(${opts.provider})`)}`
    : opts.model;

  process.stdout.write("\n");

  if (t.id === "phosphor") {
    const modelRaw = opts.provider && opts.provider !== "anthropic"
      ? `${opts.model} (${opts.provider})`
      : opts.model;
    const contentWidths = [
      9 + modelRaw.length,
      9 + repo.length,
      9 + opts.session.length,
      30 + opts.version.length,
    ];
    const W = Math.max(48, Math.max(...contentWidths) + 6);

    const pad = (s: string, raw: number): string => s + " ".repeat(Math.max(0, W - 4 - raw));
    const bc = t.boxChar;
    const top = chrome(`  ┌${bc.repeat(W - 2)}┐`);
    const bot = chrome(`  └${bc.repeat(W - 2)}┘`);
    const row = (content: string, rawLen: number): string =>
      `  ${chrome("│")} ${pad(content, rawLen)} ${chrome("│")}`;

    process.stdout.write(top + "\n");
    const title = `MONEYPENNY v${opts.version}`;
    const tag = "TERMINAL ACTIVE";
    const titleRow = `${accent(title)}${" ".repeat(Math.max(1, W - 4 - title.length - tag.length))}${chrome(tag)}`;
    process.stdout.write(row(titleRow, W - 4) + "\n");
    process.stdout.write(row(`${chrome("MODEL:")}   ${modelDisplay}`, 9 + modelRaw.length) + "\n");
    process.stdout.write(row(`${chrome("PATH:")}    ${repo}`, 9 + repo.length) + "\n");
    process.stdout.write(row(`${chrome("SESSION:")} ${opts.session}`, 9 + opts.session.length) + "\n");
    process.stdout.write(bot + "\n");
  } else if (t.id === "arcade") {
    const title = " M  O  N  E  Y  P  E  N  N  Y ";
    process.stdout.write(`  ${chrome("░▒▓")}${accent(title)}${chrome("▓▒░")}  ${chrome(`v${opts.version}`)}\n`);
    process.stdout.write(`  ${modelDisplay} ${sep} ${repo}\n`);
  } else {
    process.stdout.write(`  ${bold(accent("moneypenny"))} ${chrome(`v${opts.version}`)}\n`);
    process.stdout.write(`  ${modelDisplay} ${sep} ${repo} ${sep} ${chrome(opts.session)}\n`);
    process.stdout.write(`  ${chrome("/help for commands")} ${sep} ${chrome("/exit to quit")}\n`);
  }

  process.stdout.write("\n");
}

// ── Help ────────────────────────────────────────────────────────────────

export function printHelp(): void {
  const cmd = (name: string, args: string, desc: string): string => {
    const left = `${accent(name)}${args ? " " + chrome(args) : ""}`;
    const rawLen = name.length + (args ? 1 + args.length : 0);
    const padding = Math.max(2, 20 - rawLen);
    return `    ${left}${" ".repeat(padding)}${chrome(desc)}`;
  };

  process.stdout.write("\n");
  process.stdout.write(`  ${bold("Commands")}\n\n`);
  process.stdout.write(cmd("/model", "[name|#]", "List or switch models") + "\n");
  process.stdout.write(cmd("/theme", "[name]", "Switch visual theme") + "\n");
  process.stdout.write(cmd("/agent", "[name|#]", "List or switch agents") + "\n");
  process.stdout.write(cmd("/session", "[#|new]", "List, switch, or start sessions") + "\n");
  process.stdout.write(cmd("/search", "<q>", "Search the codebase") + "\n");
  process.stdout.write(cmd("/index", "", "Rebuild the code index") + "\n");
  process.stdout.write(cmd("/compact", "[msg]", "Compact conversation history") + "\n");
  process.stdout.write("\n");
  process.stdout.write(cmd("/summary", "", "Turns, tokens, cost, index") + "\n");
  process.stdout.write(cmd("/help", "", "Show this help") + "\n");
  process.stdout.write(cmd("/exit", "", "End session") + "\n");
  process.stdout.write("\n");
}
