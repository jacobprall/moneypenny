import type { Context } from "hono";
import { MoneypennyError, errorToJson } from "@moneypenny/core";

export function honoErrorHandler(err: Error, c: Context) {
  if (err instanceof MoneypennyError) {
    return c.json(errorToJson(err), err.status as 400);
  }
  console.error("[api] unhandled error:", err);
  return c.json(
    {
      error: {
        code: "INTERNAL",
        message: "Internal server error",
      },
    },
    500,
  );
}
