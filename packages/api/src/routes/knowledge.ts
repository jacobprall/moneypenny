import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createKnowledgeRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) =>
      c.json({
        skills: act.listSkills(ctx),
        conventions: act.listConventions(ctx),
        pointers: act.listPointers(ctx, {}),
      }),
    )
    .get("/skills", async (c) => c.json(act.listSkills(ctx)))
    .get("/conventions", async (c) => c.json(act.listConventions(ctx)))
    .get("/pointers", async (c) => {
      const sessionId = c.req.query("sessionId") ?? c.req.query("session_id") ?? undefined;
      const pinnedOnly = c.req.query("pinnedOnly") === "1" || c.req.query("pinned_only") === "1";
      return c.json(act.listPointers(ctx, { sessionId, pinnedOnly }));
    });
}
