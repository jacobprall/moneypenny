import { Hono } from "hono";
import type { ActionContext, BlueprintDirs } from "@moneypenny/core";
import { assertLocalBind } from "./auth.js";
import { buildRouter } from "./router.js";
import { honoErrorHandler } from "./error.js";

export type CreateApiDeps = {
  ctx: ActionContext;
  blueprintDirs?: BlueprintDirs;
};

export function createApi(deps: CreateApiDeps) {
  assertLocalBind();
  const api = buildRouter(deps.ctx, deps.blueprintDirs);
  const app = new Hono()
    .onError(honoErrorHandler)
    .route("/api", api);
  return { app };
}

export type AppType = ReturnType<typeof buildRouter>;

export { streamSessionEvents, streamGlobalEvents } from "./sse.js";
