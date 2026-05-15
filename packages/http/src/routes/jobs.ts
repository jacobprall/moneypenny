import { Hono } from "hono";
import { getJobById, listJobs, listRunsForJob, triggerJobById } from "@moneypenny/agents";
import { createRequireDbMiddleware, type HttpVars } from "../middleware.js";
import type { CreateHttpAppOptions } from "../types.js";

export function createJobsRouter(opts: CreateHttpAppOptions): Hono<{ Variables: HttpVars }> {
  const r = new Hono<{ Variables: HttpVars }>();
  r.use("*", createRequireDbMiddleware(opts.getDb));

  r.get("/jobs", (c) => {
    const type = c.req.query("type");
    const jobs = listJobs(c.var.db.db, type || undefined);
    return c.json({ jobs });
  });

  r.get("/jobs/:id", (c) => {
    const job = getJobById(c.var.db.db, c.req.param("id"));
    if (!job) {
      return c.json({ error: "not found" }, 404);
    }
    let payload: unknown = null;
    if (job.payload) {
      try {
        payload = JSON.parse(job.payload);
      } catch {
        payload = job.payload;
      }
    }
    return c.json({ job: { ...job, payloadParsed: payload } });
  });

  r.get("/jobs/:id/runs", (c) => {
    const id = c.req.param("id");
    if (!getJobById(c.var.db.db, id)) {
      return c.json({ error: "not found" }, 404);
    }
    const runs = listRunsForJob(c.var.db.db, id);
    return c.json({ runs });
  });

  r.post("/jobs/:id/trigger", async (c) => {
    const id = c.req.param("id");
    if (!getJobById(c.var.db.db, id)) {
      return c.json({ error: "not found" }, 404);
    }
    const getKey = opts.getApiKey ?? (() => process.env.ANTHROPIC_API_KEY);
    try {
      await triggerJobById(c.var.db, id, () => getKey());
      return c.json({ ok: true });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return c.json({ error: msg }, 500);
    }
  });

  return r;
}
