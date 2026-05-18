# Tools

A **Tool** is a typed capability an agent can invoke during a run. Tools are the concrete surface through which agents touch the world (filesystem, code index, sub-agents, the user).

## Tool Definition

```typescript
interface ToolDef<I = unknown, O = unknown> {
  name: string;                       // unique identifier
  description: string;                // shown to the LLM
  inputSchema: ZodSchema<I>;          // Zod for runtime validation + LLM JSON schema
  outputSchema?: ZodSchema<O>;        // optional, for typed results
  permissions: PermissionRequirement; // what the session must allow to use this tool
  category: 'fs' | 'code' | 'session' | 'knowledge' | 'shell' | 'meta';
  execute: (args: I, ctx: ToolContext) => Promise<O>;
}

interface PermissionRequirement {
  filesystem?: 'read' | 'readwrite';
  network?: boolean;
  shell?: boolean;
}

interface ToolContext {
  sessionId: string;
  runId: string;
  cwd: string;
  writeDb: Database;
  readDb: Database;
  events: EventBus;
  registry: BlueprintRegistry;
  runner: SessionRunner;
  abortSignal: AbortSignal;
}
```

The Zod schema serves double duty: runtime validation of LLM-produced args, and JSON-schema generation for the LLM API call (via `zod-to-json-schema`).

## Registration

```typescript
// packages/engine/src/tools/registry.ts

class ToolRegistry {
  private tools = new Map<string, ToolDef>();

  register(tool: ToolDef): void { /* ... */ }
  get(name: string): ToolDef | undefined { /* ... */ }
  list(): ToolDef[] { /* ... */ }

  /** Resolve effective tools for a session given its config + permissions. */
  resolve(sessionConfig: SessionConfig): ToolDef[] {
    const allowed = this.list().filter(t => satisfies(t.permissions, sessionConfig.permissions));
    if (!sessionConfig.tools) return allowed;
    const whitelist = new Set(sessionConfig.tools);
    return allowed.filter(t => whitelist.has(t.name));
  }
}
```

`satisfies` rejects a tool if its requirements exceed the session's grants (e.g. `tool.permissions.shell = true` but `session.permissions.shell = false`).

The registry is built at startup. Tools are imported from `packages/engine/src/tools/builtins/*` and registered explicitly in `register-builtins.ts` — no auto-discovery, no magic.

## Built-in Tools (v2)

### Filesystem (require `filesystem`)

| Name | Permission | Purpose |
|------|------------|---------|
| `read_file` | read | Read file content (cwd-relative) |
| `write_file` | readwrite | Create or overwrite file |
| `edit_file` | readwrite | Apply string-replace patch to a file |
| `list_directory` | read | List entries in a directory |

### Code (require `read`)

| Name | Permission | Purpose |
|------|------------|---------|
| `search_code` | read | Hybrid FTS + semantic search over indexed code |
| `find_symbol` | read | Find symbol by name across indexed code |
| `read_symbol` | read | Read code chunk by symbol or file+line range |

### Sessions / agents (no extra permission)

| Name | Permission | Purpose |
|------|------------|---------|
| `search_messages` | none | FTS across session messages |
| `expand_previous_session` | none | Pull messages from a labeled prior session |
| `spawn_agent` | none | Launch a child session from a blueprint |
| `request_human_input` | none | Pause and wait for user direction |
| `change_directory` | readwrite | Update session config.cwd (subject to permission re-evaluation) |

### Shell (require `shell`)

| Name | Permission | Purpose |
|------|------------|---------|
| `run_command` | shell | Execute a shell command in cwd, capture stdout/stderr |

### Knowledge (no extra permission)

| Name | Permission | Purpose |
|------|------------|---------|
| `learn_skill` | none | Record a skill for future sessions |
| `record_pointer` | none | Record a pointer in current session |
| `query_conventions` | none | List active conventions |

## Execution Flow

For each tool call requested by the LLM during a run:

1. Runtime validates name exists in resolved tool set for session
2. Runtime validates args against `inputSchema`
3. Emit `tool.started` event
4. Call `execute(args, ctx)` with `abortSignal` from the run
5. On success: emit `tool.completed`, append a `tool` role message with the result
6. On error: emit `tool.failed`, append a `tool` role message with the error string
7. Return control to the LLM with the tool result

A failed tool call does NOT fail the run; the LLM sees the error and decides what to do.

## Permission Evaluation

When a session is created, `ToolRegistry.resolve(config)` produces the effective tool set. This set is:
- Passed to the LLM as the available functions schema
- Used to validate names when the LLM picks a tool

If the session's `config` mutates mid-session (e.g. `change_directory` triggers a permission re-evaluation), the tool set is recomputed before the next run.

## Adding a New Tool

```typescript
// packages/engine/src/tools/builtins/grep.ts

export const grepTool: ToolDef<{ pattern: string }, string[]> = {
  name: 'grep',
  description: 'Search files in cwd matching a regex',
  category: 'fs',
  permissions: { filesystem: 'read' },
  inputSchema: z.object({ pattern: z.string() }),
  execute: async ({ pattern }, ctx) => {
    const proc = Bun.spawn(['rg', '-l', pattern, ctx.cwd], { signal: ctx.abortSignal });
    const text = await new Response(proc.stdout).text();
    return text.split('\n').filter(Boolean);
  },
};

// packages/engine/src/tools/register-builtins.ts
import { grepTool } from './builtins/grep';
export function registerBuiltins(reg: ToolRegistry) {
  // ... existing
  reg.register(grepTool);
}
```

That's it. The tool is now available to the LLM (subject to permissions and blueprint whitelist) and listed in `GET /tools` for the UI.

## API Surface

`GET /tools` returns the registry contents — name, description, category, permissions — for the UI's blueprint editor and permission UX. The `inputSchema` is included as JSON Schema (via `zod-to-json-schema`) for richer editors.

## Out of Scope (v2)

- User-defined tools via plugins / dynamic imports
- Remote / HTTP-backed tools
- MCP-imported tools (consuming tools from other MCP servers as if they were ours)

These are tracked but deferred. The registry interface is designed to admit them later without breaking existing tools.
