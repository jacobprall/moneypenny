import { zodToJsonSchema } from "zod-to-json-schema";
import type { ActionContext } from "./context.js";

export function listTools(ctx: ActionContext) {
  return ctx.tools.list().map((t) => ({
    name: t.name,
    description: t.description,
    category: t.category,
    permissions: t.permissions,
    inputSchema: zodToJsonSchema(t.inputSchema, { target: "openApi3" }),
  }));
}
