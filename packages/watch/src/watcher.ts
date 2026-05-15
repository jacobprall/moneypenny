/**
 * Recursive repo watcher backed by Bun/Node fs.watch — debounced with burst batching.
 */

import { existsSync, readFileSync, statSync, watch } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import type { FSWatcher } from "node:fs";
import { globMatch } from "@moneypenny/db/glob";

export interface WatcherConfig {
  repoPath: string;
  debounceMs?: number;
  excludePatterns?: string[];
  handlers: WatchHandler[];
}

export interface WatchHandler {
  name: string;
  match: (relativePath: string) => boolean;
  handle: (events: FileChangeEvent[]) => void | Promise<void>;
}

export interface FileChangeEvent {
  type: "add" | "change" | "delete";
  relativePath: string;
  absolutePath: string;
}

export interface WatcherHandle {
  stop(): void;
  stats(): WatcherStats;
}

export interface WatcherStats {
  watching: boolean;
  startedAt: number;
  eventsProcessed: number;
  lastEventAt: number | null;
  handlerStats: Record<string, number>;
}

const DEFAULT_DEBOUNCE_MS = 300;
const BURST_WINDOW_MS = 100;
const BURST_THRESHOLD = 10;

interface GitRule {
  pattern: string;
  negated: boolean;
  dirOnly: boolean;
}

function toPosixRel(fsPath: string): string {
  return fsPath.replaceAll("\\", "/");
}

function parseGitignoreContent(content: string): GitRule[] {
  const rules: GitRule[] = [];
  for (let line of content.split("\n")) {
    line = line.replace(/\r$/, "").trim();
    if (!line || line.startsWith("#")) continue;
    let negated = false;
    if (line.startsWith("!")) {
      negated = true;
      line = line.slice(1).trim();
    }
    let dirOnly = line.endsWith("/");
    if (dirOnly) line = line.slice(0, -1);
    if (line) rules.push({ pattern: line, negated, dirOnly });
  }
  return rules;
}

function rulesFromExcludePatternLines(lines: readonly string[]): GitRule[] {
  const rules: GitRule[] = [];
  for (const entry of lines) {
    const parts = entry.split("\n").map((s) => s.trim()).filter(Boolean);
    for (let line of parts) {
      if (line.startsWith("#")) continue;
      let negated = false;
      if (line.startsWith("!")) {
        negated = true;
        line = line.slice(1).trim();
      }
      let dirOnly = line.endsWith("/");
      if (dirOnly) line = line.slice(0, -1);
      if (line) rules.push({ pattern: line, negated, dirOnly });
    }
  }
  return rules;
}

function gitIgnored(normRelPath: string, isDir: boolean, rules: readonly GitRule[]): boolean {
  let ignored = false;
  const norm = normRelPath.replaceAll("\\", "/");
  for (const r of rules) {
    if (r.dirOnly && !isDir) continue;
    const target = norm;
    if (globMatch(r.pattern, target)) ignored = !r.negated;
  }
  return ignored;
}

function isRepoRootIgnoreMetaFile(rel: string): boolean {
  const n = rel.replaceAll("\\", "/");
  return n === ".gitignore" || n === ".mpignore";
}

function loadIgnoreRules(
  repoPathAbs: string,
  excludePatterns?: string[],
): GitRule[] {
  const collected: GitRule[] = [];

  const gi = join(repoPathAbs, ".gitignore");
  if (existsSync(gi)) {
    try {
      collected.push(...parseGitignoreContent(readFileSync(gi, "utf8")));
    } catch {
      /* keep previous */
    }
  }

  const mp = join(repoPathAbs, ".mpignore");
  if (existsSync(mp)) {
    try {
      collected.push(...parseGitignoreContent(readFileSync(mp, "utf8")));
    } catch {
      /* keep previous */
    }
  }

  if (excludePatterns?.length) collected.push(...rulesFromExcludePatternLines(excludePatterns));

  return collected;
}

function isExcluded(repoPathAbs: string, relativePath: string, rules: readonly GitRule[]): boolean {
  let isDir = false;
  try {
    isDir = statSync(join(repoPathAbs, relativePath)).isDirectory();
  } catch {
    isDir = false;
  }
  return gitIgnored(relativePath, isDir, rules);
}

type MutableEvent = { type: FileChangeEvent["type"]; relativePath: string; absolutePath: string };

function mergeEventPriority(a: FileChangeEvent["type"], b: FileChangeEvent["type"]): FileChangeEvent["type"] {
  if (a === "delete" || b === "delete") return "delete";
  if (a === "change" || b === "change") return "change";
  return "add";
}

function resolveWatcherEntry(
  repoPathAbs: string,
  fileNameBuf: Buffer | string | null | undefined,
  eventKind: string,
): FileChangeEvent | null {
  if (fileNameBuf == null || fileNameBuf === "") return null;

  const fileName =
    typeof fileNameBuf === "string"
      ? fileNameBuf
      : Buffer.isBuffer(fileNameBuf)
        ? fileNameBuf.toString("utf8")
        : String(fileNameBuf);
  const normalizedName = sep !== "/" ? fileName.replaceAll("\\", "/") : fileName;

  const absolutePath = resolve(join(repoPathAbs, normalizedName));
  const relativePath = toPosixRel(relative(repoPathAbs, absolutePath));
  if (!relativePath || relativePath.startsWith("..") || relativePath === "..") return null;

  let type: FileChangeEvent["type"];
  if (eventKind === "change") {
    type = "change";
  } else {
    type = existsSync(absolutePath) ? "add" : "delete";
  }

  return { type, relativePath, absolutePath };
}

export function startWatcher(config: WatcherConfig): WatcherHandle {
  const debounceMs = config.debounceMs ?? DEFAULT_DEBOUNCE_MS;
  const repoPathAbs = resolve(config.repoPath);
  let ignoreRules = loadIgnoreRules(repoPathAbs, config.excludePatterns);

  const pending = new Map<string, MutableEvent>();
  let eventsProcessed = 0;
  const handlerStats: Record<string, number> = {};
  for (const h of config.handlers) handlerStats[h.name] ??= 0;

  let watching = false;
  const startedAt = Date.now();
  let lastEventAt: number | null = null;

  const burstRecents: { t: number; pathKey: string }[] = [];

  let burstMode = false;
  const debounceTimers = new Map<string, ReturnType<typeof setTimeout>>();
  let flushBatchTimer: ReturnType<typeof setTimeout> | null = null;

  function pruneBurst(now: number): void {
    while (burstRecents.length > 0 && now - burstRecents[0]!.t > BURST_WINDOW_MS) {
      burstRecents.shift();
    }
  }

  function recentUniqueCount(now: number): number {
    pruneBurst(now);
    return new Set(burstRecents.map((x) => x.pathKey)).size;
  }

  function reloadRulesIfIgnoresTouch(relPaths: Iterable<string>): void {
    for (const p of relPaths) {
      if (isRepoRootIgnoreMetaFile(p)) {
        ignoreRules = loadIgnoreRules(repoPathAbs, config.excludePatterns);
        return;
      }
    }
  }

  async function dispatchToHandlers(events: FileChangeEvent[]): Promise<void> {
    if (events.length === 0) return;
    reloadRulesIfIgnoresTouch(events.map((e) => e.relativePath));

    const filtered = events.filter((e) => !isExcluded(repoPathAbs, e.relativePath, ignoreRules));

    eventsProcessed += events.length;
    for (const handler of config.handlers) {
      const matched = filtered.filter((e) => handler.match(e.relativePath));
      if (matched.length === 0) continue;

      handlerStats[handler.name] = (handlerStats[handler.name] ?? 0) + matched.length;
      await Promise.resolve(handler.handle(matched));
    }
  }

  function buildEventsPayload(): FileChangeEvent[] {
    const out: FileChangeEvent[] = [];
    for (const m of pending.values()) {
      out.push({ type: m.type, relativePath: m.relativePath, absolutePath: m.absolutePath });
    }
    pending.clear();
    return out;
  }

  async function flushBatch(): Promise<void> {
    flushBatchTimer = null;
    burstMode = false;
    burstRecents.length = 0;
    debounceTimers.forEach((t) => clearTimeout(t));
    debounceTimers.clear();

    await dispatchToHandlers(buildEventsPayload());
  }

  async function flushOnePath(pathKey: string): Promise<void> {
    if (burstMode) return;
    debounceTimers.delete(pathKey);
    const event = pending.get(pathKey);
    if (!event) return;
    pending.delete(pathKey);
    await dispatchToHandlers([{ type: event.type, relativePath: event.relativePath, absolutePath: event.absolutePath }]);
  }

  function scheduleBurstFlush(): void {
    burstMode = true;
    debounceTimers.forEach((t) => clearTimeout(t));
    debounceTimers.clear();
    if (flushBatchTimer) clearTimeout(flushBatchTimer);
    flushBatchTimer = setTimeout(() => {
      void flushBatch();
    }, debounceMs);
  }

  function scheduleDebounced(evt: FileChangeEvent): void {
    const pathKey = evt.relativePath;
    const prev = pending.get(pathKey);

    pending.set(pathKey, {
      type: prev ? mergeEventPriority(prev.type, evt.type) : evt.type,
      relativePath: evt.relativePath,
      absolutePath: evt.absolutePath,
    });

    const now = Date.now();
    burstRecents.push({ t: now, pathKey });
    pruneBurst(now);

    if (burstMode) {
      scheduleBurstFlush();
      return;
    }

    if (recentUniqueCount(now) > BURST_THRESHOLD) {
      scheduleBurstFlush();
      return;
    }

    const existingDeb = debounceTimers.get(pathKey);
    if (existingDeb) clearTimeout(existingDeb);

    debounceTimers.set(
      pathKey,
      setTimeout(() => {
        void flushOnePath(pathKey);
      }, debounceMs),
    );
  }

  const watcherMaybe = watch(
    repoPathAbs,
    { recursive: true },
    (eventType, fileNameBuf) => {
      const evt = resolveWatcherEntry(repoPathAbs, fileNameBuf ?? null, String(eventType));
      if (!evt) return;

      lastEventAt = Date.now();
      scheduleDebounced(evt);
    },
  );

  const watcher: FSWatcher = watcherMaybe;
  watching = true;

  return {
    stop(): void {
      watching = false;
      burstMode = false;
      watcher.close();
      debounceTimers.forEach((t) => clearTimeout(t));
      debounceTimers.clear();
      if (flushBatchTimer) clearTimeout(flushBatchTimer);
      flushBatchTimer = null;
      pending.clear();
      burstRecents.length = 0;
    },

    stats(): WatcherStats {
      return {
        watching,
        startedAt,
        eventsProcessed,
        lastEventAt,
        handlerStats: { ...handlerStats },
      };
    },
  };
}
