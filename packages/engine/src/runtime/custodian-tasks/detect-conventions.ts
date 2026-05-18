import type { Database } from "bun:sqlite";
import type { EventBus } from "../../events/index.js";
import { detectConventions } from "../../conventions.js";

export async function detectConventionsTask(db: Database, events?: EventBus): Promise<number> {
  const count = await detectConventions(db);
  if (events && count > 0) {
    events.emit({ type: "knowledge.convention_detected", detail: { count } });
  }
  return count;
}
