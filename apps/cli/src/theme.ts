export type ColorSpec = [r: number, g: number, b: number, fallback: string];

export interface ThemeColors {
  accent: ColorSpec;
  data: ColorSpec;
  success: ColorSpec;
  error: ColorSpec;
  warning: ColorSpec;
  chrome: ColorSpec;
}

export interface Theme {
  id: string;
  label: string;
  colors: ThemeColors;
  spinner: string[];
  toolMarker: string;
  pipe: string;
  corner: string;
  ruleChar: string;
  codeOpen: string;
  codePipe: string;
  codeRuleChar: string;
  separator: string;
  boxChar: string;
  costTokenFormat: "verbose" | "compact";
  turnLabel: string;
  errorTag: string;
  warnTag: string;
  thinkingText: string;
  sessionHeader: string;
  sessionLatest: string;
  sessionFreshLabel: string;
}

// ── Modern (default) ────────────────────────────────────────────────────

const modern: Theme = {
  id: "modern",
  label: "Modern",
  colors: {
    accent: [94, 214, 148, "32"],
    data: [130, 190, 230, "36"],
    success: [72, 199, 142, "32"],
    error: [235, 87, 87, "31"],
    warning: [240, 185, 80, "33"],
    chrome: [108, 118, 128, "2"],
  },
  spinner: ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
  toolMarker: "▸",
  pipe: "│",
  corner: "╰",
  ruleChar: "─",
  codeOpen: "╭──",
  codePipe: "│",
  codeRuleChar: "─",
  separator: "─",
  boxChar: "─",
  costTokenFormat: "verbose",
  turnLabel: "turn",
  errorTag: "[ERR]",
  warnTag: "[!!]",
  thinkingText: "Thinking...",
  sessionHeader: "Sessions:",
  sessionLatest: "← latest",
  sessionFreshLabel: "f for a fresh session",
};

// ── Phosphor (70s CRT terminal) ─────────────────────────────────────────

const phosphor: Theme = {
  id: "phosphor",
  label: "Phosphor",
  colors: {
    accent: [255, 176, 0, "33"],
    data: [232, 160, 48, "33"],
    success: [255, 200, 0, "33"],
    error: [255, 80, 40, "31"],
    warning: [255, 200, 80, "33"],
    chrome: [139, 115, 64, "2"],
  },
  spinner: ["|", "/", "─", "\\"],
  toolMarker: "▸",
  pipe: "┃",
  corner: "╹",
  ruleChar: "═",
  codeOpen: "┌══",
  codePipe: "┃",
  codeRuleChar: "═",
  separator: "═",
  boxChar: "═",
  costTokenFormat: "verbose",
  turnLabel: "RUN",
  errorTag: "[ERR]",
  warnTag: "[!!]",
  thinkingText: "PROCESSING...",
  sessionHeader: "SESSIONS:",
  sessionLatest: "← LATEST",
  sessionFreshLabel: "F FOR NEW SESSION",
};

// ── Arcade (neon cabinet) ───────────────────────────────────────────────

const arcade: Theme = {
  id: "arcade",
  label: "Arcade",
  colors: {
    accent: [255, 45, 149, "35"],
    data: [0, 255, 255, "36"],
    success: [0, 255, 128, "32"],
    error: [255, 50, 50, "31"],
    warning: [255, 230, 0, "33"],
    chrome: [140, 120, 160, "2"],
  },
  spinner: ["░", "▒", "▓", "█", "▓", "▒"],
  toolMarker: "▶",
  pipe: "┃",
  corner: "╰",
  ruleChar: "─",
  codeOpen: "╔══",
  codePipe: "║",
  codeRuleChar: "═",
  separator: "▀▄",
  boxChar: "═",
  costTokenFormat: "compact",
  turnLabel: "ROUND",
  errorTag: "✖ ERR",
  warnTag: "⚠ !!",
  thinkingText: "LOADING...",
  sessionHeader: "HIGH SCORES:",
  sessionLatest: "← LAST PLAY",
  sessionFreshLabel: "F FOR NEW GAME",
};

// ── Registry ────────────────────────────────────────────────────────────

export const THEMES: Record<string, Theme> = { modern, phosphor, arcade };
export const THEME_NAMES = Object.keys(THEMES);

let _active: Theme = modern;

export function getTheme(): Theme {
  return _active;
}

export function setTheme(name: string): void {
  const t = THEMES[name];
  if (!t) throw new Error(`Unknown theme: ${name}`);
  _active = t;
}

export function isThemeName(s: string): s is string {
  return s in THEMES;
}
