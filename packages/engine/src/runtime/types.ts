import type { Database } from "bun:sqlite";
import type { BlueprintRegistry } from "../blueprints/registry.js";
import type { EventBus } from "../events/index.js";
import type {
  PermissionRequirement,
  Permissions,
  SessionRunner as ToolSessionRunner,
} from "../tools/types.js";
import type { ToolRegistry } from "../tools/registry.js";

export type RuntimeDeps = {
  writeDb: Database;
  readDb: Database;
  events: EventBus;
  blueprints: BlueprintRegistry;
  tools: ToolRegistry;
};

export type LaunchAgentForScheduleFn = (input: {
  blueprint: string;
  task: string;
  cwd?: string;
  label?: string;
}) => Promise<{ sessionId: string }>;

export type SchedulerDeps = RuntimeDeps & {
  repoRoot: string;
  launchScheduledAgent: LaunchAgentForScheduleFn;
};

export type CustodianDeps = RuntimeDeps & {
  archiveAfterDays: number;
  compactMessageThreshold: number;
  eventRetentionDays: number;
};

export type WorkLoopDeps = RuntimeDeps & {
  batchSize: number;
  onFullReindex?: () => Promise<void>;
};

export type WatcherDeps = {
  codeOnChange: (path: string) => void;
  codeOnRemove: (path: string) => void;
  configDirs: string[];
  configOnChange: (path: string, event: string) => void;
};

export type StoredSessionConfig = {
  cwd: string;
  blueprint: string;
  model?: string;
  strategy?: "autonomous" | "hitl" | "review";
  permissions: {
    filesystem: "read" | "readwrite";
    network: boolean;
    shell: boolean;
  };
  tools: string[] | null;
  pause_after: string[];
  max_turns: number;
  context: { conventions: boolean; skills: string[] };
  instructions: string;
};

export function parseSessionConfig(raw: string): StoredSessionConfig | null {
  try {
    const j = JSON.parse(raw) as Partial<StoredSessionConfig>;
    if (typeof j.cwd !== "string") return null;
    if (typeof j.blueprint !== "string") return null;
    if (!j.permissions || typeof j.permissions !== "object") return null;
    if (!("instructions" in j) || typeof j.instructions !== "string") return null;
    return {
      cwd: j.cwd,
      blueprint: j.blueprint,
      model: j.model,
      strategy: j.strategy === "hitl" || j.strategy === "review" ? j.strategy : "autonomous",
      permissions: {
        filesystem:
          j.permissions.filesystem === "readwrite" ? "readwrite" : "read",
        network: !!j.permissions.network,
        shell: !!j.permissions.shell,
      },
      tools: j.tools ?? null,
      pause_after: Array.isArray(j.pause_after)
        ? j.pause_after.filter((x): x is string => typeof x === "string")
        : [],
      max_turns: typeof j.max_turns === "number" ? j.max_turns : 50,
      context: {
        conventions: j.context?.conventions !== false,
        skills: Array.isArray(j.context?.skills)
          ? j.context!.skills!.filter((s): s is string => typeof s === "string")
          : [],
      },
      instructions: j.instructions,
    };
  } catch {
    return null;
  }
}

export function intersectChildPermissions(
  parent: Permissions,
  requested?: PermissionRequirement,
): StoredSessionConfig["permissions"] {
  const req = requested ?? {};
  const wantFs = req.filesystem === "readwrite" ? "readwrite" : "read";
  const fs: "read" | "readwrite" =
    wantFs === "readwrite" && parent.filesystem === "readwrite"
      ? "readwrite"
      : parent.filesystem === "readwrite" && wantFs === "read"
        ? "read"
        : "read";
  return {
    filesystem: fs,
    network: !!req.network && parent.network,
    shell: !!req.shell && parent.shell,
  };
}

export function intersectChildTools(
  parentTools: string[] | null,
  childTools: string[] | null,
  allowedNames: Set<string>,
): string[] | null {
  const parentEff =
    parentTools == null
      ? null
      : parentTools.filter((t) => allowedNames.has(t));
  if (childTools == null && parentEff == null) return null;
  const base =
    parentEff ?? (childTools == null ? null : [...allowedNames]);
  if (childTools == null) return base;
  const pr = base ?? [...allowedNames];
  const out = childTools.filter((t) => pr.includes(t));
  return out.length ? out : [];
}

export type { ToolSessionRunner };
