import type { ProviderName } from "@moneypenny/loop";
import {
  closeAgentDB,
  closeWorkspaceDB,
  createSession,
  getActiveSession,
  getConfig,
  getPermissions,
  scanSkillDirs,
  setActiveSession,
  syncPolicyFiles,
  type Permission,
} from "@moneypenny/db";
import { getIndexStatus, indexCodebase } from "@moneypenny/search";
import {
  credentialRedactor,
  dbPolicyHook,
  toolGovernance,
  type GovernanceConfig,
} from "@moneypenny/ctx";
import { createToolRegistry, registerBuiltinTools } from "@moneypenny/tools";
import { Command } from "commander";
import * as path from "node:path";

import { success, Spinner, printBanner, printDebug, printError, printInfo, muted } from "../display.js";
import { resolveConfig, readGlobalConfig } from "../config.js";
import { isThemeName, setTheme } from "../theme.js";
import { createDefaultPrompt } from "../prompt.js";
import { ensureAgentDefaults, migrateToSingleDb, openAgent, openWorkspace } from "../session.js";
import { resolveAgentInteractively } from "../pickers.js";
import { runRepl, runPiped } from "../repl.js";

function permissionsToGovernance(permissions: Permission[]): GovernanceConfig {
  const allowedTools: string[] = [];
  const deniedTools: string[] = [];
  const pathAllow: string[] = [];
  const pathDeny: string[] = [];

  for (const p of permissions) {
    switch (p.type) {
      case "tool_allow": allowedTools.push(p.pattern); break;
      case "tool_deny": deniedTools.push(p.pattern); break;
      case "path_allow": pathAllow.push(p.pattern); break;
      case "path_deny": pathDeny.push(p.pattern); break;
    }
  }

  const config: GovernanceConfig = {};
  if (allowedTools.length > 0) config.allowedTools = allowedTools;
  if (deniedTools.length > 0) config.deniedTools = deniedTools;
  if (pathAllow.length > 0 || pathDeny.length > 0) {
    config.pathRestrictions = {};
    if (pathAllow.length > 0) config.pathRestrictions.allow = pathAllow;
    if (pathDeny.length > 0) config.pathRestrictions.deny = pathDeny;
  }
  return config;
}

export const chatCommand = new Command("chat")
  .description("Start or resume an interactive agent session")
  .argument("[message]", "Initial message to send (skips first prompt)")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--agent <name>", "Agent name (default: interactive picker)")
  .option("--session <id>", "Resume a specific session ID")
  .option("--new", "Create new agent with fresh session")
  .option("--model <model>", "Override model")
  .option("--provider <provider>", "LLM provider (anthropic, openai, google)")
  .option("--no-index", "Skip index freshness check")
  .option("--no-confirm", "Skip tool confirmation prompts")
  .action(
    async (message: string | undefined, opts: {
      repo: string;
      agent?: string;
      session?: string;
      new?: boolean;
      model?: string;
      provider?: string;
      index?: boolean;
      confirm?: boolean;
    }) => {
      let db: ReturnType<typeof openAgent> | undefined;
      let workspace: ReturnType<typeof openWorkspace> | undefined;

      const savedTheme = readGlobalConfig("theme");
      if (savedTheme && isThemeName(savedTheme)) setTheme(savedTheme);

      try {
        const repoPath = path.resolve(opts.repo);

        const migration = migrateToSingleDb(repoPath);
        if (migration.migrated) {
          printInfo(muted(`  Migrated ${String(migration.agents.length)} agent DB${migration.agents.length === 1 ? "" : "s"} to mp.db (backups in .mp/agents.backup/)`));
        }

        ensureAgentDefaults(repoPath);
        const config = resolveConfig({
          model: opts.model,
          provider: opts.provider as ProviderName | undefined,
          ...(typeof opts.confirm === "boolean" ? { confirmDestructive: opts.confirm } : {}),
        });

        const interactive = process.stdin.isTTY ?? false;
        workspace = openWorkspace(repoPath);

        // 1. Resolve agent + session
        const resolved = interactive
          ? await resolveAgentInteractively(repoPath, workspace, opts)
          : { agentName: opts.agent ?? "default", startFreshSession: Boolean(opts.new), explicitSessionId: opts.session };

        db = openAgent(repoPath, { name: resolved.agentName, blueprint: resolved.blueprint, workspace });

        if (resolved.explicitSessionId) {
          setActiveSession(db, resolved.explicitSessionId);
        } else if (resolved.startFreshSession) {
          const session = createSession(db, undefined, resolved.agentName);
          setActiveSession(db, session.id);
          printDebug(`New session started (${session.id.slice(0, 8)})`);
        } else {
          const existing = getActiveSession(db);
          if (existing) {
            setActiveSession(db, existing.id);
          } else {
            const session = createSession(db, undefined, resolved.agentName);
            setActiveSession(db, session.id);
          }
        }

        // 2. Index
        if (opts.index !== false && config.autoIndex) {
          const status = getIndexStatus(db);
          if (status.totalChunks === 0) {
            const sp = new Spinner();
            sp.start("Building initial code index...");
            const result = indexCodebase(db, repoPath);
            sp.stop();
            process.stdout.write(
              `  ${success("\u2714")} Indexed ${String(result.filesScanned)} files, ${String(result.chunksCreated)} chunks in ${(result.elapsedMs / 1000).toFixed(1)}s\n`,
            );
          } else {
            const result = indexCodebase(db, repoPath);
            if (result.filesChanged > 0) {
              printDebug(`Index refreshed: ${String(result.filesChanged)} files updated`);
            }
          }
        }

        // 3. Build tools + hooks (confirmation gate added by repl.ts, not here)
        const registry = createToolRegistry();
        registerBuiltinTools(registry);
        const toolDefs = registry.listForLLM();

        const governanceConfig = permissionsToGovernance(getPermissions(db));
        const maxTurnsRaw = getConfig(db, "max_turns");
        const maxIterations =
          maxTurnsRaw != null && Number.isFinite(Number(maxTurnsRaw)) && Number(maxTurnsRaw) > 0
            ? Number(maxTurnsRaw) : undefined;

        const hooks = [
          credentialRedactor(),
          dbPolicyHook({ db: () => db!.db }),
          toolGovernance(governanceConfig),
        ];

        const userSkillsDir = path.join(repoPath, ".mp", "skills");
        const bundledSkillsDir = path.resolve(import.meta.dir, "../../../packages/skills/bundled");
        scanSkillDirs(db, [
          { dir: bundledSkillsDir, source: "builtin" },
          { dir: userSkillsDir, source: "user" },
        ]);

        syncPolicyFiles(db, path.join(repoPath, ".mp", "policies"));

        const prompt = createDefaultPrompt(toolDefs);

        // 4. Run
        const replConfig = {
          db,
          repoPath,
          agentName: resolved.agentName,
          model: config.model,
          provider: config.provider,
          apiKey: config.apiKey,
          hooks,
          prompt,
          registry,
          maxIterations,
          maxCostPerSession: config.maxCostPerSession,
          confirmDestructive: config.confirmDestructive && interactive,
          initialMessage: message,
        };

        if (!interactive) {
          await runPiped(replConfig);
        } else {
          printBanner({
            version: "0.1.0",
            session: db.activeSessionId?.slice(0, 8) ?? resolved.agentName,
            model: config.model,
            provider: config.provider,
            repoPath,
          });
          await runRepl(replConfig);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exitCode = 1;
      } finally {
        if (db) try { closeAgentDB(db); } catch { /* best effort */ }
        if (workspace) try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
      }
    },
  );
