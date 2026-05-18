import {
  createSession,
  getSession,
  insertPendingUserMessage,
  updateSessionConfigOptimistic,
} from "@moneypenny/db";
import type { Blueprint } from "../blueprints/types.js";
import type { SessionRunner as ToolSessionRunner } from "../tools/types.js";
import {
  effectivePermissions,
  type PermissionRequirement,
} from "../tools/types.js";
import { AgentLoop } from "./agent-loop.js";
import { getToolCallingSession, getToolCallingRunId } from "./tool-context.js";
import {
  intersectChildPermissions,
  intersectChildTools,
  parseSessionConfig,
  type RuntimeDeps,
  type StoredSessionConfig,
} from "./types.js";

type Deps = RuntimeDeps;

const MAX_SPAWN_DEPTH = 5;

export class EngineSessionRunner implements ToolSessionRunner {
  private readonly loops = new Map<string, AgentLoop>();

  constructor(private readonly deps: Deps) {}

  activeSessionIds(): string[] {
    return [...this.loops.keys()];
  }

  async launch(sessionId: string, initialMessage?: string): Promise<void> {
    if (initialMessage) {
      insertPendingUserMessage(this.deps.writeDb, sessionId, initialMessage);
    }
    if (this.loops.has(sessionId)) return;
    const loop = new AgentLoop(sessionId, {
      ...this.deps,
      runner: this,
      sessionOps: {
        setCwd: (sid, cwd) => {
          const s = getSession(this.deps.writeDb, sid);
          if (!s) return;
          const cfg = parseSessionConfig(s.config);
          if (!cfg) return;
          cfg.cwd = cwd;
          updateSessionConfigOptimistic(
            this.deps.writeDb,
            sid,
            JSON.stringify(cfg),
            s.config_version,
          );
        },
      },
    });
    this.loops.set(sessionId, loop);
    void loop.run().finally(() => {
      this.loops.delete(sessionId);
    });
  }

  async inject(sessionId: string, content: string): Promise<void> {
    insertPendingUserMessage(this.deps.writeDb, sessionId, content);
    const session = getSession(this.deps.readDb, sessionId);
    if (!session) return;
    if (session.status === "paused" || session.status === "active") {
      void this.launch(sessionId);
    }
  }

  async pause(sessionId: string): Promise<void> {
    const loop = this.loops.get(sessionId);
    if (loop) await loop.pause();
  }

  async resume(sessionId: string): Promise<void> {
    const session = getSession(this.deps.readDb, sessionId);
    if (session?.status === "paused") {
      this.deps.events.emit({
        type: "hitl.resumed",
        session_id: sessionId,
        detail: {},
      });
    }
    await this.launch(sessionId);
  }

  async kill(sessionId: string): Promise<void> {
    const loop = this.loops.get(sessionId);
    if (loop) await loop.abort();
  }

  async launchChild(input: {
    blueprint: string;
    task: string;
    label?: string;
    cwd?: string;
    permissions?: PermissionRequirement;
    tools?: string[] | null;
  }): Promise<{ sessionId: string }> {
    const parentId = getToolCallingSession() ?? this.fallbackActiveSessionId();
    const p = getSession(this.deps.readDb, parentId);
    if (!p) throw new Error("launchChild: parent session not found");

    const depth = this.getSpawnDepth(parentId);
    if (depth >= MAX_SPAWN_DEPTH) {
      this.deps.events.emit({
        type: "child.failed",
        session_id: parentId,
        detail: {
          reason: "max_depth_exceeded",
          depth,
          max: MAX_SPAWN_DEPTH,
        },
      });
      throw new Error(
        `Spawn depth limit reached (${depth}/${MAX_SPAWN_DEPTH}). Cannot spawn more children.`,
      );
    }

    const parentCfg = parseSessionConfig(p.config);
    if (!parentCfg) throw new Error("launchChild: invalid parent config");

    const bp =
      this.deps.blueprints.resolve(input.blueprint, input.cwd) ??
      this.deps.blueprints.getDefault();

    const parentGrant = effectivePermissions({
      permissions: parentCfg.permissions,
      tools: parentCfg.tools,
    });
    const bpPerms = intersectChildPermissions(parentGrant, bp.permissions);
    const childPerms = intersectChildPermissions(
      { filesystem: bpPerms.filesystem, network: bpPerms.network, shell: bpPerms.shell },
      input.permissions,
    );
    const allowed = this.deps.tools.resolve({
      permissions: childPerms,
      tools: null,
    });
    const allowedNames = new Set(allowed.map((t) => t.name));
    const tools = intersectChildTools(
      parentCfg.tools,
      input.tools ?? null,
      allowedNames,
    );

    const configObj = this.snapshotConfig(bp, {
      cwd: input.cwd ?? parentCfg.cwd,
      permissions: childPerms,
      tools,
    });

    const child = createSession(this.deps.writeDb, {
      label: input.label ?? null,
      parentId: p.id,
      config: JSON.stringify(configObj),
    });

    const parentRunId = getToolCallingRunId();
    this.deps.events.emit({
      type: "child.spawned",
      session_id: p.id,
      run_id: parentRunId,
      detail: { child_id: child.id, blueprint: bp.name, parent_run_id: parentRunId ?? "" },
    });
    await this.launch(child.id, input.task);
    return { sessionId: child.id };
  }

  private getSpawnDepth(sessionId: string): number {
    let depth = 0;
    let current = sessionId;
    while (depth < MAX_SPAWN_DEPTH + 1) {
      const row = this.deps.readDb
        .query<{ parent_id: string | null }, [string]>(
          `SELECT parent_id FROM sessions WHERE id = ?`,
        )
        .get(current);
      if (!row?.parent_id) break;
      depth++;
      current = row.parent_id;
    }
    return depth;
  }

  private fallbackActiveSessionId(): string {
    const keys = [...this.loops.keys()];
    if (keys.length === 0) throw new Error("launchChild: no active session");
    return keys[keys.length - 1]!;
  }

  private snapshotConfig(
    bp: Blueprint,
    overrides: {
      cwd: string;
      permissions: StoredSessionConfig["permissions"];
      tools: string[] | null;
    },
  ): StoredSessionConfig {
    return {
      cwd: overrides.cwd,
      blueprint: bp.name,
      model: bp.model,
      strategy: bp.strategy,
      permissions: overrides.permissions,
      tools: overrides.tools,
      pause_after: bp.pause_after,
      max_turns: bp.max_turns,
      context: bp.context,
      instructions: bp.body,
    };
  }
}
