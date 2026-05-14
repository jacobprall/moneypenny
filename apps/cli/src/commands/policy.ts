import { Command } from "commander";
import * as path from "node:path";
import { closeAgentDB, closeWorkspaceDB, createPolicy, deletePolicy, listPolicies, syncPolicyFiles } from "@swe/db";
import { openSession, openWorkspace } from "../session";
import { printError } from "../display";

export const policyCommand = new Command("policy").description("Manage governance policies in the local database");

policyCommand
  .command("list")
  .description("List all policies")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .action((opts: { repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      console.log(JSON.stringify(listPolicies(db), null, 2));
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });

policyCommand
  .command("add")
  .description("Add a policy")
  .requiredOption("--name <name>", "Policy name")
  .requiredOption("--effect <effect>", "allow | deny | audit | confirm")
  .option("--priority <n>", "Priority (higher first)", "0")
  .option("--tool <pattern>", "Tool glob pattern")
  .option("--path <pattern>", "Path glob pattern")
  .option("--actor <pattern>", "Actor glob pattern")
  .option("--message <text>", "Human-readable reason")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .action(
    (opts: {
      name: string;
      effect: string;
      priority: string;
      tool?: string;
      path?: string;
      actor?: string;
      message?: string;
      repo: string;
      session: string;
    }) => {
      const repoPath = path.resolve(opts.repo);
      const workspace = openWorkspace(repoPath);
      const db = openSession(repoPath, { session: opts.session, workspace });
      try {
        const effect = opts.effect as "allow" | "deny" | "audit" | "confirm";
        if (!["allow", "deny", "audit", "confirm"].includes(effect)) {
          printError("effect must be allow, deny, audit, or confirm");
          process.exitCode = 1;
          return;
        }
        const p = createPolicy(db, {
          name: opts.name,
          effect,
          priority: parseInt(opts.priority, 10) || 0,
          toolPattern: opts.tool ?? null,
          pathPattern: opts.path ?? null,
          costCondition: null,
          argsPattern: null,
          actorPattern: opts.actor ?? null,
          message: opts.message ?? null,
          enabled: 1,
        });
        console.log(JSON.stringify(p, null, 2));
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exitCode = 1;
      } finally {
        try { closeAgentDB(db); } catch { /* best effort */ }
        try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
      }
    },
  );

policyCommand
  .command("remove")
  .description("Delete a policy by id")
  .argument("<id>", "Policy id")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .action((id: string, opts: { repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      deletePolicy(db, id);
      process.stdout.write(`Removed policy ${id}\n`);
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });

policyCommand
  .command("sync")
  .description("Sync .swe/policies/*.yaml files into the database")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .action((opts: { repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      const policiesDir = path.join(repoPath, ".swe", "policies");
      const result = syncPolicyFiles(db, policiesDir);
      process.stdout.write(
        `Synced: ${String(result.added)} added, ${String(result.updated)} updated, ${String(result.removed)} removed\n`,
      );
      if (result.errors.length > 0) {
        for (const e of result.errors) {
          process.stderr.write(`  error: ${e.file} — ${e.message}\n`);
        }
      }
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });
