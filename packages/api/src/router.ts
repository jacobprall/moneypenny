import { Hono } from "hono";
import type { ActionContext, BlueprintDirs } from "@moneypenny/core";
import { createAgentsRoutes } from "./routes/agents.js";
import { createBlueprintsRoutes } from "./routes/blueprints.js";
import { createCodeRoutes } from "./routes/code.js";
import { createEventsRoutes } from "./routes/events.js";
import { createFilesRoutes } from "./routes/files.js";
import { createIdeasRoutes } from "./routes/ideas.js";
import { createKnowledgeRoutes } from "./routes/knowledge.js";
import { createMessagesRoutes } from "./routes/messages.js";
import { createRunsRoutes } from "./routes/runs.js";
import { createSessionsRoutes } from "./routes/sessions.js";
import { createSystemRoutes } from "./routes/system.js";
import { createTabsRoutes } from "./routes/tabs.js";
import { createToolsRoutes } from "./routes/tools.js";
import { createSseRoutes } from "./routes/sse-routes.js";
import { honoErrorHandler } from "./error.js";

export function buildRouter(
  ctx: ActionContext,
  blueprintDirs?: BlueprintDirs,
) {
  return new Hono()
    .onError(honoErrorHandler)
    .route("/sessions", createSessionsRoutes(ctx))
    .route("/messages", createMessagesRoutes(ctx))
    .route("/runs", createRunsRoutes(ctx))
    .route("/blueprints", createBlueprintsRoutes(ctx, blueprintDirs))
    .route("/ideas", createIdeasRoutes(ctx))
    .route("/agents", createAgentsRoutes(ctx))
    .route("/tools", createToolsRoutes(ctx))
    .route("/code", createCodeRoutes(ctx))
    .route("/files", createFilesRoutes(ctx))
    .route("/knowledge", createKnowledgeRoutes(ctx))
    .route("/events", createEventsRoutes(ctx))
    .route("/tabs", createTabsRoutes(ctx))
    .route("/system", createSystemRoutes(ctx))
    .route("/sse", createSseRoutes(ctx));
}
