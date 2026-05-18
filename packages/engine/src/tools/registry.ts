import {
  effectivePermissions,
  satisfiesRequirement,
  type SessionConfig,
  type ToolDef,
} from "./types.js";

export class ToolRegistry {
  private tools = new Map<string, ToolDef>();

  register(tool: ToolDef): void {
    this.tools.set(tool.name, tool);
  }

  get(name: string): ToolDef | undefined {
    return this.tools.get(name);
  }

  list(): ToolDef[] {
    return [...this.tools.values()];
  }

  resolve(sessionConfig: SessionConfig): ToolDef[] {
    const grant = effectivePermissions(sessionConfig);
    const allowed = this.list().filter((t) =>
      satisfiesRequirement(t.permissions, grant),
    );
    if (!sessionConfig.tools) return allowed;
    const whitelist = new Set(sessionConfig.tools);
    return allowed.filter((t) => whitelist.has(t.name));
  }
}
