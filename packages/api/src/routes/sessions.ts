import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

const createBody = z.object({
  blueprint: z.string().optional(),
  cwd: z.string(),
  label: z.string().nullable().optional(),
  parentId: z.string().nullable().optional(),
  ideaId: z.string().nullable().optional(),
  task: z.string().optional(),
});

const injectBody = z.object({ content: z.string() });

const patchSession = z.object({
  label: z.string().optional(),
  status: z.enum(["active", "paused", "completed", "archived"]).optional(),
});

const VALID_TRANSITIONS: Record<string, string[]> = {
  active: ["running", "paused", "completed", "archived", "failed"],
  running: ["active", "paused", "completed", "failed"],
  paused: ["active", "running", "completed", "archived"],
  completed: ["active", "archived"],
  failed: ["active", "archived"],
  archived: [],
};

function isValidTransition(from: string, to: string): boolean {
  return VALID_TRANSITIONS[from]?.includes(to) ?? false;
}

const patchConfig = z.object({
  config: z.string(),
  version: z.number().int().optional(),
});

export function createSessionsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => {
      const status = c.req.query("status") ?? undefined;
      const blueprint = c.req.query("blueprint") ?? undefined;
      const label = c.req.query("label") ?? undefined;
      const cursorRaw = c.req.query("cursor");
      const limitRaw = c.req.query("limit");
      const cursorN = cursorRaw ? Number(cursorRaw) : null;
      const limitN = limitRaw ? Number(limitRaw) : undefined;
      const out = act.listSessions(ctx, {
        status,
        blueprint,
        label,
        cursor: cursorN != null && Number.isFinite(cursorN) ? cursorN : null,
        limit: limitN != null && Number.isFinite(limitN) ? Math.min(Math.max(limitN, 1), 200) : undefined,
      });
      return c.json(out);
    })
    .get("/:id", async (c) =>
      c.json(act.getSessionDetail(ctx, c.req.param("id"))),
    )
    .post("/", zValidator("json", createBody), async (c) =>
      c.json(await act.createSession(ctx, c.req.valid("json"))),
    )
    .post("/:id/inject", zValidator("json", injectBody), async (c) => {
      await act.injectMessage(ctx, c.req.param("id"), c.req.valid("json").content);
      return c.json({ ok: true });
    })
    .patch("/:id", zValidator("json", patchSession), async (c) => {
      const id = c.req.param("id");
      const body = c.req.valid("json");
      const session = act.getSessionDetail(ctx, id);
      if (!session) return c.json({ error: { code: "NOT_FOUND", message: id } }, 404);
      const s = "session" in session ? session.session : session;
      if (body.status !== undefined) {
        const currentStatus = (s as { status: string }).status;
        if (!isValidTransition(currentStatus, body.status)) {
          return c.json(
            {
              error: {
                code: "INVALID_TRANSITION",
                message: `Cannot transition from "${currentStatus}" to "${body.status}"`,
              },
            },
            409,
          );
        }
      }
      if (body.label !== undefined) {
        ctx.writeDb.query(`UPDATE sessions SET label = ? WHERE id = ?`).run(body.label, id);
      }
      if (body.status !== undefined) {
        ctx.writeDb.query(`UPDATE sessions SET status = ? WHERE id = ?`).run(body.status, id);
      }
      const updated = act.getSessionDetail(ctx, id);
      const u = updated && "session" in updated ? updated.session : updated;
      return c.json(u);
    })
    .patch(
      "/:id/config",
      zValidator("json", patchConfig),
      async (c) => {
        const ifMatch = c.req.header("If-Match");
        const b = c.req.valid("json");
        const ver = b.version ?? (ifMatch ? Number(ifMatch) : NaN);
        if (Number.isNaN(ver))
          return c.json({ error: { code: "VALIDATION_FAILED", message: "version required" } }, 400);
        const out = act.updateSessionConfig(ctx, c.req.param("id"), b.config, ver);
        return c.json(out);
      },
    )
    .post("/:id/pause", async (c) => {
      await act.pauseSession(ctx, c.req.param("id"));
      return c.json({ ok: true });
    })
    .post("/:id/resume", async (c) => {
      await act.resumeSession(ctx, c.req.param("id"));
      return c.json({ ok: true });
    })
    .post("/:id/complete", async (c) => {
      act.completeSession(ctx, c.req.param("id"));
      return c.json({ ok: true });
    })
    .post("/:id/archive", async (c) => {
      act.archiveSession(ctx, c.req.param("id"));
      return c.json({ ok: true });
    })
    .delete("/:id", async (c) => {
      act.deleteSession(ctx, c.req.param("id"));
      return c.json({ ok: true });
    });
}
