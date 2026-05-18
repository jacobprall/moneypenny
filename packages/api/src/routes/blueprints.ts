import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";
import type { BlueprintDirs } from "@moneypenny/core";

export function createBlueprintsRoutes(
  ctx: ActionContext,
  dirs?: BlueprintDirs,
) {
  return new Hono()
    .get("/", async (c) => c.json(act.listBlueprints(ctx)))
    .get("/:name", async (c) =>
      c.json(act.getBlueprint(ctx, c.req.param("name"))),
    )
    .post("/reload", async (c) => {
      if (!dirs)
        return c.json({ error: { code: "INTERNAL", message: "dirs missing" } }, 500);
      act.reloadBlueprints(ctx, dirs);
      return c.json({ ok: true });
    });
}
