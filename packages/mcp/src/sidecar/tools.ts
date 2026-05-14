import type { SearchHit } from "./client.js";
import type { SidecarClient } from "./client.js";

export async function runCodeSearch(
  client: SidecarClient,
  args: { query: string; limit?: number; languages?: string[]; paths?: string[] },
): Promise<{ hits: SearchHit[]; text: string }> {
  const hits = await client.codeSearch(args.query, {
    limit: args.limit ?? 15,
    languages: args.languages,
    paths: args.paths,
  });
  const text =
    hits.length === 0
      ? "No results."
      : hits
          .map(
            (h) =>
              `${h.path}:${h.startLine}-${h.endLine} (score: ${h.score.toFixed(2)})\n${h.chunkText}`,
          )
          .join("\n\n");
  return { hits, text };
}
