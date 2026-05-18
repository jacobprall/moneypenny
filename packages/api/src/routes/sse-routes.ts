import { Hono } from "hono";
import type { ActionContext } from "@moneypenny/core";
import { streamGlobalEvents, streamSessionEvents } from "../sse.js";

export function createSseRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/sessions/:id", (c) => streamSessionEvents(c, ctx))
    .get("/events", (c) => streamGlobalEvents(c, ctx));
}

/** @deprecated Use createSseRoutes instead */
export function registerSseRoutes(app: Hono, ctx: ActionContext): void {
  app.get("/sse/sessions/:id", (c) => streamSessionEvents(c, ctx));
  app.get("/sse/events", (c) => streamGlobalEvents(c, ctx));
}
