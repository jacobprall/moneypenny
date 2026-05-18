import { streamText, tool, type CoreMessage } from "ai";
import {
  appendAssistantMessage,
  appendToolResultMessage,
  drainPending,
  getSession,
  updateSessionStatus,
  type Message,
} from "@moneypenny/db";
import {
  abortRun,
  failRun,
  finishRun,
  startRun,
  type Run,
} from "@moneypenny/db";
import { calculateCost } from "../cost.js";
import { modelForTier, resolveModel } from "../llm.js";
import type { ToolRegistry } from "../tools/registry.js";
import type { SessionRunner as ToolSessionRunner } from "../tools/types.js";
import {
  effectivePermissions,
  type SessionConfig,
  type ToolContext,
} from "../tools/types.js";
import { buildV2SystemPrompt, messagesToCore } from "./agent-loop-prompt.js";
import { dispatchTool } from "./dispatch-tool.js";
import type { RuntimeDeps, StoredSessionConfig } from "./types.js";
import { parseSessionConfig } from "./types.js";
import {
  getToolCallingSession,
  getToolCallingRunId,
  setToolCallingSession,
  setToolCallingRunId,
} from "./tool-context.js";

type AgentLoopDeps = RuntimeDeps & {
  runner: ToolSessionRunner;
  sessionOps?: ToolContext["sessionOps"];
};

const CHECKPOINT_RE = /\[\[checkpoint:\s*([^\]]+?)]]/g;
const DEFAULT_TOKEN_BUDGET = 200_000;
const BUDGET_RATIO = 0.7;

function isSqliteAiModel(modelStr: string): boolean {
  return modelStr.startsWith("sqliteai:");
}

export class AgentLoop {
  private abortCtl = new AbortController();
  private pauseRequested = false;
  lastRunPaused = false;

  constructor(
    private readonly sessionId: string,
    private readonly deps: AgentLoopDeps,
  ) {}

  async pause(): Promise<void> {
    this.pauseRequested = true;
  }

  async abort(): Promise<void> {
    this.abortCtl.abort();
  }

  async run(): Promise<void> {
    const { writeDb, readDb, events } = this.deps;
    try {
      while (!this.abortCtl.signal.aborted) {
        const pendingRows = readDb
          .query<Message, [string]>(
            `SELECT * FROM messages WHERE session_id = ? AND pending = 1 ORDER BY seq ASC`,
          )
          .all(this.sessionId);
        if (pendingRows.length > 0) {
          drainPending(writeDb, this.sessionId);
          for (const m of pendingRows) {
            events.emit({
              type: "message.user",
              session_id: this.sessionId,
              detail: { message_id: m.id },
            });
          }
        }

        const session = getSession(readDb, this.sessionId);
        if (!session) return;
        const cfg = parseSessionConfig(session.config);
        if (!cfg) {
          const run = startRun(writeDb, {
            sessionId: this.sessionId,
            model: null,
            blueprint: null,
          });
          failRun(writeDb, run.id, "invalid session config");
          updateSessionStatus(writeDb, this.sessionId, "failed");
          return;
        }

        const shouldRun = pendingRows.length > 0 || this.needsAssistantTurn();
        if (!shouldRun) break;

        updateSessionStatus(writeDb, this.sessionId, "running");
        const run = startRun(writeDb, {
          sessionId: this.sessionId,
          model: cfg.model ?? modelForTier("strong"),
          blueprint: cfg.blueprint,
        });
        events.emit({
          type: "run.started",
          session_id: this.sessionId,
          run_id: run.id,
          detail: { model: cfg.model ?? modelForTier("strong"), blueprint: cfg.blueprint },
        });
        this.lastRunPaused = false;
        this.pauseRequested = false;

        try {
          await this.executeRun(cfg, run);
        } catch (err) {
          failRun(writeDb, run.id, String(err));
          updateSessionStatus(writeDb, this.sessionId, "failed");
          events.emit({
            type: "run.failed",
            session_id: this.sessionId,
            run_id: run.id,
            detail: { error: String(err) },
          });
          events.emit({
            type: "session.failed",
            session_id: this.sessionId,
            detail: { error: String(err), last_run_id: run.id },
          });
          return;
        }

        if (this.lastRunPaused) {
          updateSessionStatus(writeDb, this.sessionId, "paused");
          break;
        }

        if (cfg.strategy === "review") {
          updateSessionStatus(writeDb, this.sessionId, "paused");
          break;
        }
      }
      if (!this.abortCtl.signal.aborted) {
        const st = getSession(readDb, this.sessionId)?.status;
        if (st === "running")
          updateSessionStatus(writeDb, this.sessionId, "active");
      }
    } finally {
      this.abortCtl = new AbortController();
    }
  }

  private needsAssistantTurn(): boolean {
    const rows = this.deps.readDb
      .query<{ role: string }, [string]>(
        `SELECT role FROM messages WHERE session_id = ? ORDER BY seq DESC LIMIT 12`,
      )
      .all(this.sessionId);
    if (rows.length === 0) return false;
    const last = rows[0]!;
    return last.role === "user" || last.role === "tool";
  }

  private loadCompactionAwareHistory(): Message[] {
    const compactMarker = this.deps.readDb
      .query<{ seq: number }, [string]>(
        `SELECT seq FROM messages
         WHERE session_id = ? AND role = 'system' AND content LIKE '[compact] %'
         ORDER BY seq DESC LIMIT 1`,
      )
      .get(this.sessionId);

    if (compactMarker) {
      return this.deps.readDb
        .query<Message, [string, number]>(
          `SELECT * FROM messages WHERE session_id = ? AND seq >= ? ORDER BY seq ASC`,
        )
        .all(this.sessionId, compactMarker.seq);
    }

    return this.deps.readDb
      .query<Message, [string]>(
        `SELECT * FROM messages WHERE session_id = ? ORDER BY seq DESC LIMIT 120`,
      )
      .all(this.sessionId)
      .reverse();
  }

  private async executeRun(cfg: StoredSessionConfig, run: Run): Promise<void> {
    const runStart = Date.now();
    const modelStr = cfg.model ?? modelForTier("strong");
    if (isSqliteAiModel(modelStr)) {
      throw new Error(
        "sqliteai: models are not supported in the streaming agent loop; set session model to a cloud or ollama provider",
      );
    }

    const sys = buildV2SystemPrompt(this.deps.readDb, cfg);
    const hist = this.loadCompactionAwareHistory();
    const core: CoreMessage[] = messagesToCore(hist);

    const reviewMode = cfg.strategy === "review";
    const sessConf: SessionConfig = {
      permissions: reviewMode
        ? { filesystem: "read", network: false, shell: false }
        : cfg.permissions,
      tools: cfg.tools,
    };
    const resolved = this.deps.tools.resolve(sessConf);
    const runControl: ToolContext["runControl"] = {
      lastRunPaused: false,
      permissionsNeedReeval: false,
    };

    const aiTools = this.buildAiTools(
      resolved,
      run,
      runControl,
    ) as Parameters<typeof streamText>[0]["tools"];

    const assistantRow = appendAssistantMessage(this.deps.writeDb, {
      sessionId: this.sessionId,
      runId: run.id,
      content: "",
    });
    this.deps.events.emit({
      type: "message.assistant.started",
      session_id: this.sessionId,
      run_id: run.id,
      detail: { message_id: assistantRow.id },
    });

    let buf = "";
    let lastFlush = Date.now();
    const flush = (force: boolean): void => {
      if (!force && Date.now() - lastFlush < 50 && buf.length < 400) return;
      this.deps.writeDb
        .query<unknown, [string, string]>(
          `UPDATE messages SET content = ? WHERE id = ?`,
        )
        .run(buf, assistantRow.id);
      lastFlush = Date.now();
    };

    let hitCheckpoint: string | undefined;
    const onText = (delta: string): void => {
      buf += delta;
      let m: RegExpExecArray | null;
      const r = new RegExp(CHECKPOINT_RE);
      while ((m = r.exec(buf)) !== null) {
        const name = m[1]!.trim();
        if (cfg.pause_after.includes(name)) hitCheckpoint = name;
      }
      this.deps.events.emit({
        type: "message.assistant.token",
        session_id: this.sessionId,
        run_id: run.id,
        detail: { message_id: assistantRow.id, content: delta },
      });
      flush(false);
    };

    const result = streamText({
      model: resolveModel(modelStr),
      system: sys,
      messages: core,
      tools: aiTools,
      maxSteps: Math.max(1, cfg.max_turns),
      abortSignal: this.abortCtl.signal,
      onChunk: ({ chunk }) => {
        if (chunk.type === "text-delta" && chunk.textDelta) {
          onText(chunk.textDelta);
        }
      },
    });

    try {
      for await (const _ of result.textStream) {
        if (this.pauseRequested) {
          this.abortCtl.abort();
          break;
        }
      }
    } catch {
      /* aborted */
    }

    buf = (await result.text) ?? buf;
    flush(true);
    const usage = await result.usage;
    const costUsd = calculateCost(modelStr, {
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
    });
    finishRun(this.deps.writeDb, run.id, {
      tokensIn: usage.promptTokens,
      tokensOut: usage.completionTokens,
      costUsd,
    });

    const tokenLimit = DEFAULT_TOKEN_BUDGET * BUDGET_RATIO;
    if (usage.promptTokens > tokenLimit) {
      this.lastRunPaused = true;
      this.deps.events.emit({
        type: "budget.exceeded",
        session_id: this.sessionId,
        run_id: run.id,
        detail: {
          prompt_tokens: usage.promptTokens,
          limit: tokenLimit,
          reason: "budget_exceeded",
        },
      });
    } else if (usage.promptTokens > tokenLimit * 0.85) {
      this.deps.events.emit({
        type: "budget.warned",
        session_id: this.sessionId,
        run_id: run.id,
        detail: {
          prompt_tokens: usage.promptTokens,
          limit: tokenLimit,
        },
      });
    }

    const toolCallsJson = await result.toolCalls;
    if (toolCallsJson?.length) {
      this.deps.writeDb
        .query<unknown, [string, string]>(
          `UPDATE messages SET tool_calls = ? WHERE id = ?`,
        )
        .run(JSON.stringify(toolCallsJson), assistantRow.id);
    }

    this.deps.events.emit({
      type: "message.assistant.completed",
      session_id: this.sessionId,
      run_id: run.id,
      detail: {
        message_id: assistantRow.id,
        has_tool_calls: !!toolCallsJson?.length,
      },
    });

    if (hitCheckpoint) {
      this.lastRunPaused = true;
      this.deps.events.emit({
        type: "hitl.checkpoint",
        session_id: this.sessionId,
        run_id: run.id,
        detail: { checkpoint_name: hitCheckpoint },
      });
    }

    if (runControl.lastRunPaused) this.lastRunPaused = true;

    const steps = await result.steps;
    if (steps.length >= cfg.max_turns) this.lastRunPaused = true;

    if (this.pauseRequested) {
      abortRun(this.deps.writeDb, run.id, "paused");
      this.lastRunPaused = true;
      this.deps.events.emit({
        type: "run.aborted",
        session_id: this.sessionId,
        run_id: run.id,
        detail: { reason: "paused" },
      });
      return;
    }

    this.deps.events.emit({
      type: "run.completed",
      session_id: this.sessionId,
      run_id: run.id,
      detail: {
        tokens_in: usage.promptTokens,
        tokens_out: usage.completionTokens,
        cost_usd: costUsd,
        duration_ms: Date.now() - runStart,
      },
    });
  }

  private buildAiTools(
    resolved: ReturnType<ToolRegistry["resolve"]>,
    run: Run,
    runControl: ToolContext["runControl"],
  ): Record<string, ReturnType<typeof tool>> {
    const out: Record<string, ReturnType<typeof tool>> = {};
    for (const td of resolved) {
      out[td.name] = tool({
        description: td.description,
        parameters: td.inputSchema,
        execute: async (args, { toolCallId }) => {
            const prev = getToolCallingSession();
            const prevRun = getToolCallingRunId();
            setToolCallingSession(this.sessionId);
            setToolCallingRunId(run.id);
            try {
            const cur = getSession(this.deps.readDb, this.sessionId);
            const parsedCfg = cur ? parseSessionConfig(cur.config) : null;
            const cwd = parsedCfg?.cwd ?? process.cwd();
            const ctx: ToolContext = {
              sessionId: this.sessionId,
              runId: run.id,
              cwd,
              writeDb: this.deps.writeDb,
              readDb: this.deps.readDb,
              events: this.deps.events,
              registry: this.deps.blueprints,
              runner: this.deps.runner,
              abortSignal: this.abortCtl.signal,
              runControl,
              sessionOps: this.deps.sessionOps,
            };
            void effectivePermissions({
              permissions: parsedCfg?.permissions ?? {
                filesystem: "read",
                network: false,
                shell: false,
              },
              tools: parsedCfg?.tools ?? null,
            });
            const res = await dispatchTool(
              { id: toolCallId, name: td.name, args },
              ctx,
              this.deps.tools,
            );
            const text =
              "ok" in res && res.ok
                ? JSON.stringify(res.value)
                : JSON.stringify(res);
            const tm = appendToolResultMessage(this.deps.writeDb, {
              sessionId: this.sessionId,
              runId: run.id,
              toolCallId,
              content: text,
            });
            this.deps.events.emit({
              type: "message.tool.result",
              session_id: this.sessionId,
              run_id: run.id,
              detail: {
                message_id: tm.id,
                tool_call_id: toolCallId,
                tool_name: td.name,
              },
            });
            if ("ok" in res && res.ok) return res.value;
            return { error: res.error, message: res.message };
          } finally {
            setToolCallingSession(prev);
            setToolCallingRunId(prevRun);
          }
        },
      });
    }
    return out;
  }
}
