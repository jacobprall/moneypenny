import * as readline from "node:readline";
import { createAgentLoop, createChildLoopFactory, runAutoLabel, type AgentLoop, type ProviderName } from "@moneypenny/loop";
import { createSession, getConfig, getActiveSession, setActiveSession, createLocalGen, type AgentDB, type LocalGen } from "@moneypenny/db";
import type { ToolRegistry } from "@moneypenny/tools";
import { confirmationGate, createHookPipeline, type Hook, type Prompt } from "@moneypenny/ctx";
import { extractSessionKnowledge } from "@moneypenny/skills";
import { accent, muted, printError, printInfo, printTurnSeparator } from "./display.js";
import { handleSlashCommand, type SlashContext } from "./slash-commands.js";
import { resolveConfig } from "./config.js";
import { EventRenderer } from "./event-renderer.js";

export interface ReplConfig {
  db: AgentDB;
  repoPath: string;
  agentName: string;
  initialMessage?: string;

  model: string;
  provider: ProviderName;
  apiKey: string;
  hooks: Hook[];
  prompt: Prompt;
  registry: ToolRegistry;
  maxIterations?: number;
  maxCostPerSession?: number;
  confirmDestructive: boolean;
}

/**
 * Wrap readline.question as a promise that resolves to null on EOF/close.
 * Only one question is ever in-flight -- the REPL loop serializes naturally.
 */
function question(rl: readline.Interface, prompt: string): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    let done = false;
    const onClose = (): void => {
      if (!done) { done = true; resolve(null); }
    };
    rl.once("close", onClose);
    rl.question(prompt, (answer) => {
      if (!done) {
        done = true;
        rl.removeListener("close", onClose);
        resolve(answer);
      }
    });
  });
}

function buildHookPipeline(
  hooks: Hook[],
  rl: readline.Interface,
  confirmDestructive: boolean,
): ReturnType<typeof createHookPipeline> {
  const hookList = [...hooks];
  if (confirmDestructive) {
    hookList.push(confirmationGate({
      requireConfirmation: ["bash", "file_write", "file_edit", "git_commit", "web_fetch", "web_search"],
      promptFn: (toolName: string, input: unknown) => {
        const preview =
          typeof input === "object" && input !== null
            ? JSON.stringify(input).slice(0, 200)
            : String(input);
        const prompt = `  ${accent(toolName)} ${muted(preview)}\n  ${muted("approve?")} [y/N] `;
        return new Promise<boolean>((resolve) => {
          rl.question(prompt, (ans) => {
            const t = ans.trim().toLowerCase();
            resolve(t === "y" || t === "yes");
          });
        });
      },
    }));
  }
  return createHookPipeline(hookList, {
    hookTimeoutMs: confirmDestructive ? 0 : undefined,
  });
}

async function buildLoop(cfg: ReplConfig, pipeline: ReturnType<typeof createHookPipeline>): Promise<AgentLoop> {
  return createAgentLoop({
    model: cfg.model,
    apiKey: cfg.apiKey,
    provider: cfg.provider,
    tools: cfg.registry,
    hooks: pipeline,
    ctx: cfg.prompt,
    repoPath: cfg.repoPath,
    maxIterations: cfg.maxIterations,
    maxCostPerSession: cfg.maxCostPerSession,
    childLoopFactory: createChildLoopFactory({
      model: cfg.model,
      apiKey: cfg.apiKey,
      provider: cfg.provider,
      parentRegistry: cfg.registry,
    }),
  });
}

async function runTurn(
  loop: AgentLoop,
  db: AgentDB,
  message: string,
  renderer: EventRenderer,
): Promise<void> {
  printTurnSeparator();
  process.stdout.write("\n");
  try {
    for await (const event of loop.run(db, message)) {
      renderer.handle(event);
    }
  } catch (e) {
    renderer.stop();
    printError(e instanceof Error ? e.message : String(e));
  }
}

async function maybeExtractKnowledge(cfg: ReplConfig, localGen?: LocalGen): Promise<void> {
  const enabled = getConfig(cfg.db, "extract_on_session_end");
  if (enabled === "false" || enabled === "0") return;

  const extractModel = getConfig(cfg.db, "extract_model");

  try {
    const result = await extractSessionKnowledge(cfg.db, {
      apiKey: cfg.apiKey,
      model: extractModel ?? undefined,
      localGen,
    });
    if (result && result.skillsUpserted > 0) {
      printInfo(
        muted(`  Learned ${String(result.skillsUpserted)} skill${result.skillsUpserted === 1 ? "" : "s"} from this session: ${result.skillNames.join(", ")}`),
      );
    }
  } catch (e) {
    printError(`Knowledge extraction failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

/**
 * The interactive REPL. Uses a simple async for-loop instead of
 * recursive callbacks -- this eliminates readline conflicts, module-level
 * globals, and the "closes after 1 turn" class of bugs.
 */
export async function runRepl(cfg: ReplConfig): Promise<void> {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  const renderer = new EventRenderer();
  const pipeline = buildHookPipeline(cfg.hooks, rl, cfg.confirmDestructive);
  let loop = await buildLoop(cfg, pipeline);

  const localGen = createLocalGen();

  let { model: activeModel, provider: activeProvider, apiKey: activeApiKey } = cfg;
  let activeAgentName = cfg.agentName;

  const activeDb = cfg.db;
  const slashCtx: SlashContext = { db: activeDb, repoPath: cfg.repoPath };

  function sessionLabel(): string {
    const session = getActiveSession(activeDb);
    return session?.label ?? session?.id.slice(0, 8) ?? "—";
  }

  function statusLine(): string {
    const sep = muted("·");
    return `  ${muted(activeAgentName)} ${sep} ${muted(sessionLabel())} ${sep} ${muted(activeModel)}`;
  }

  process.once("SIGINT", () => {
    renderer.stop();
    process.stdout.write("\n");
    rl.close();
  });

  void runAutoLabel({
    repoPath: cfg.repoPath,
    model: activeModel,
    provider: activeProvider,
    apiKey: activeApiKey,
    localGen,
  });

  try {
    if (cfg.initialMessage) {
      await runTurn(loop, activeDb, cfg.initialMessage, renderer);
    }

    for (;;) {
      process.stdout.write(`\n${statusLine()}\n`);
      const input = await question(rl, `  ${accent(">")} `);
      if (input === null) break;
      const trimmed = input.trim();
      if (!trimmed) continue;
      if (trimmed === "/exit" || trimmed === "/quit") break;

      if (trimmed.startsWith("/")) {
        try {
          const result = await handleSlashCommand(trimmed, slashCtx);
          if (result && "switchModel" in result && result.switchModel) {
            const { model: newModel, provider: newProv } = result.switchModel;
            try {
              const newConfig = resolveConfig({ model: newModel, provider: newProv });
              activeModel = newConfig.model;
              activeProvider = newConfig.provider;
              activeApiKey = newConfig.apiKey;
              loop = await buildLoop({
                ...cfg,
                db: activeDb,
                model: activeModel,
                provider: activeProvider,
                apiKey: activeApiKey,
              }, pipeline);
              printInfo(muted(`  Now using ${activeModel} via ${activeProvider}`));
            } catch (e) {
              printError(e instanceof Error ? e.message : String(e));
            }
          }
          if (result && "newSession" in result && result.newSession) {
            const session = createSession(activeDb, undefined, activeAgentName);
            setActiveSession(activeDb, session.id);
            printInfo(muted(`  Fresh session started (${session.id.slice(0, 8)})`));
          }
          if (result && "switchSession" in result && result.switchSession) {
            setActiveSession(activeDb, result.switchSession.sessionId);
            loop = await buildLoop({
              ...cfg,
              db: activeDb,
              model: activeModel,
              provider: activeProvider,
              apiKey: activeApiKey,
            }, pipeline);
          }
        } catch (e) {
          printError(e instanceof Error ? e.message : String(e));
        }
        continue;
      }

      await runTurn(loop, activeDb, trimmed, renderer);
    }
  } finally {
    renderer.stop();
    rl.close();
    await maybeExtractKnowledge({ ...cfg, db: activeDb }, localGen);
    localGen.close();
  }
}

/**
 * Non-interactive piped mode: read all of stdin, run one turn, exit.
 */
export async function runPiped(cfg: ReplConfig): Promise<void> {
  const renderer = new EventRenderer();
  const pipeline = createHookPipeline(cfg.hooks);
  const loop = await buildLoop(cfg, pipeline);

  const chunks: string[] = [];
  process.stdin.setEncoding("utf8");
  for await (const chunk of process.stdin) {
    chunks.push(chunk as string);
  }
  const piped = chunks.join("").trim();
  if (!piped) {
    printError("No input received from stdin.");
    process.exit(1);
  }

  try {
    for await (const event of loop.run(cfg.db, piped)) {
      renderer.handle(event);
    }
  } catch (e) {
    renderer.stop();
    printError(e instanceof Error ? e.message : String(e));
    process.exitCode = 1;
  } finally {
    await maybeExtractKnowledge(cfg);
  }
}
