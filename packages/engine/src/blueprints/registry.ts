import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Database } from "bun:sqlite";
import CronExpressionParser from "cron-parser";
import { upsertSchedule, disableScheduleByBlueprint } from "@moneypenny/db";
import type { EventBus } from "../events/index.js";
import { parseBlueprint } from "./parse.js";
import type { Blueprint } from "./types.js";
import type { BlueprintSource } from "./types.js";
import { validateBlueprint } from "./validate.js";

export type FileWatcherHandle = {
  on(
    event: "add" | "change" | "unlink",
    handler: (path: string) => void,
  ): void;
  close(): Promise<void> | void;
};

/** Pluggable watcher (e.g. chokidar from the CLI) — avoids a hard engine dependency. */
export type FileWatcherFn = (paths: string | string[]) => FileWatcherHandle;

function listMd(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => f.endsWith(".md"))
    .map((f) => join(dir, f));
}

const FALLBACK: Blueprint = {
  name: "default",
  tools: [
    "read_file",
    "list_directory",
    "search_code",
    "find_symbol",
    "read_symbol",
    "request_human_input",
  ],
  permissions: { filesystem: "read", network: false, shell: false },
  strategy: "hitl",
  pause_after: [],
  max_turns: 50,
  context: { conventions: true, skills: [] },
  trigger_on: "manual",
  body: "You are a careful coding agent.",
  path: "",
  source: "global",
};

export class BlueprintRegistry {
  private items = new Map<string, Blueprint>();
  private globalDir = "";
  private repoDir = "";
  private handle?: FileWatcherHandle;

  constructor(
    private readonly opts: {
      watch: FileWatcherFn;
      events?: EventBus;
      writeDb?: Database;
    },
  ) {}

  start(globalDir: string, repoDir?: string): void {
    this.globalDir = globalDir;
    this.repoDir = repoDir ?? "";
    void this.handle?.close?.();
    this.rebuildAll();
    const paths = [globalDir, ...(repoDir ? [repoDir] : [])].filter((p) =>
      existsSync(p),
    );
    if (!paths.length) return;
    this.handle = this.opts.watch(paths);
    for (const ev of ["add", "change", "unlink"] as const) {
      this.handle.on(ev, (p) => this.onFs(ev, p));
    }
  }

  private rebuildAll(): void {
    this.items.clear();
    for (const p of listMd(this.globalDir)) this.ingest(p, "global");
    if (this.repoDir) {
      for (const p of listMd(this.repoDir)) this.ingest(p, "repo");
    }
  }

  private ingest(abs: string, source: BlueprintSource): void {
    const parsed = parseBlueprint(abs, source);
    if ("error" in parsed) {
      this.opts.events?.emit({
        type: "blueprint.invalid",
        detail: { path: abs, errors: [parsed.error] },
      });
      return;
    }
    const v = validateBlueprint(parsed);
    if (!v.ok) {
      this.opts.events?.emit({
        type: "blueprint.invalid",
        detail: { path: abs, errors: v.errors },
      });
      return;
    }
    this.items.set(v.blueprint.name, v.blueprint);
    this.syncSchedule(v.blueprint);
    this.opts.events?.emit({
      type: "blueprint.loaded",
      detail: { name: v.blueprint.name, path: abs },
    });
  }

  private onFs(ev: "add" | "change" | "unlink", p: string): void {
    if (!p.endsWith(".md")) return;
    if (ev === "unlink") {
      for (const [k, b] of this.items) {
        if (b.path === p) {
          this.items.delete(k);
          this.removeSchedule(k);
          this.opts.events?.emit({
            type: "blueprint.removed",
            detail: { name: k, path: p },
          });
        }
      }
      return;
    }
    const source: BlueprintSource =
      this.repoDir && p.startsWith(this.repoDir) ? "repo" : "global";
    this.ingest(p, source);
  }

  private syncSchedule(bp: Blueprint): void {
    const db = this.opts.writeDb;
    if (!db) return;
    if (bp.trigger_on === "schedule" && bp.schedule) {
      try {
        const now = Math.floor(Date.now() / 1000);
        const expr = CronExpressionParser.parse(bp.schedule, {
          currentDate: new Date(now * 1000),
          tz: "UTC",
        });
        const nextRunAt = Math.floor(expr.next().getTime() / 1000);
        upsertSchedule(db, {
          id: `bp:${bp.name}`,
          blueprint: bp.name,
          cron_expr: bp.schedule,
          enabled: 1,
          next_run_at: nextRunAt,
        });
      } catch {
        this.opts.events?.emit({
          type: "blueprint.invalid",
          detail: { path: bp.path, errors: [`Invalid cron: ${bp.schedule}`] },
        });
      }
    } else {
      disableScheduleByBlueprint(db, bp.name);
    }
  }

  private removeSchedule(blueprintName: string): void {
    const db = this.opts.writeDb;
    if (!db) return;
    disableScheduleByBlueprint(db, blueprintName);
  }

  reload(): void {
    this.rebuildAll();
  }

  async stop(): Promise<void> {
    await this.handle?.close?.();
    this.handle = undefined;
  }

  resolve(name: string, _cwd?: string): Blueprint | undefined {
    return this.items.get(name) ?? (name === "default" ? this.getDefault() : undefined);
  }

  getDefault(): Blueprint {
    const hit = this.items.get("default");
    if (hit) return hit;
    try {
      const path = fileURLToPath(new URL("./default.md", import.meta.url));
      const p = parseBlueprint(path, "global");
      if (!("error" in p)) {
        const v = validateBlueprint(p);
        if (v.ok) return v.blueprint;
      }
    } catch {
      /* use FALLBACK */
    }
    return FALLBACK;
  }

  list(): Blueprint[] {
    return [...this.items.values()];
  }
}
