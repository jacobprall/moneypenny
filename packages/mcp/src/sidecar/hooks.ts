import type { SidecarClient } from "./client.js";

export async function appendCodeContext(
  client: SidecarClient,
  query: string,
  opts?: { limit?: number },
): Promise<string | null> {
  if (!query.trim()) return null;
  const ok = await client.health();
  if (!ok) return null;
  const hits = await client.codeSearch(query, { limit: opts?.limit ?? 5 });
  if (hits.length === 0) return null;
  const lines = hits.map(
    (h) => `${h.path}:${h.startLine}-${h.endLine} (score: ${h.score.toFixed(2)})\n${h.chunkText.slice(0, 1500)}`,
  );
  return ["## Repository context", "", ...lines].join("\n\n");
}

export async function evaluateActionPolicy(
  client: SidecarClient,
  input: { actor: string; action: string; resource: string; sessionId?: string },
): Promise<{ allow: boolean; reason: string }> {
  const decision = await client.evaluatePolicy(input);
  if (!decision) {
    return { allow: true, reason: "policy service unavailable" };
  }
  if (decision.effect === "deny") {
    return { allow: false, reason: decision.reason };
  }
  return { allow: true, reason: decision.reason };
}
