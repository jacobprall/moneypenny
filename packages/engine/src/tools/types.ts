import type { ZodType } from "zod";
import type { Database } from "bun:sqlite";
import type { EventBus } from "../events/index.js";
import type { BlueprintRegistry } from "../blueprints/registry.js";

export interface PermissionRequirement {
  filesystem?: "read" | "readwrite";
  network?: boolean;
  shell?: boolean;
}

export interface Permissions {
  filesystem: "read" | "readwrite";
  network: boolean;
  shell: boolean;
}

export interface SessionRunner {
  launchChild(input: {
    blueprint: string;
    task: string;
    label?: string;
    cwd?: string;
    permissions?: PermissionRequirement;
    tools?: string[] | null;
  }): Promise<{ sessionId: string }>;
}

export interface ToolContext {
  sessionId: string;
  runId: string;
  cwd: string;
  writeDb: Database;
  readDb: Database;
  events: EventBus;
  registry: BlueprintRegistry;
  runner: SessionRunner;
  abortSignal: AbortSignal;
  runControl: {
    lastRunPaused: boolean;
    permissionsNeedReeval: boolean;
  };
  /** Optional hook for mutating session config (database layer supplies implementation). */
  sessionOps?: {
    setCwd(sessionId: string, cwd: string): void;
  };
}

export interface ToolDef<I = unknown, O = unknown> {
  name: string;
  description: string;
  inputSchema: ZodType<I>;
  outputSchema?: ZodType<O>;
  permissions: PermissionRequirement;
  category: "fs" | "code" | "session" | "knowledge" | "shell" | "meta";
  execute: (args: I, ctx: ToolContext) => Promise<O>;
}

export interface SessionConfig {
  permissions: Partial<Permissions>;
  tools: string[] | null;
}

export function satisfiesRequirement(
  req: PermissionRequirement,
  grant: Permissions,
): boolean {
  if (req.filesystem === "read") {
    if (grant.filesystem !== "read" && grant.filesystem !== "readwrite") {
      return false;
    }
  }
  if (req.filesystem === "readwrite" && grant.filesystem !== "readwrite") {
    return false;
  }
  if (req.network && !grant.network) return false;
  if (req.shell && !grant.shell) return false;
  return true;
}

export function effectivePermissions(config: SessionConfig): Permissions {
  return {
    filesystem: config.permissions.filesystem ?? "read",
    network: config.permissions.network ?? false,
    shell: config.permissions.shell ?? false,
  };
}
