import { z } from "zod";
import type { ToolDefinition } from "../types.js";
import { truncate } from "../utils.js";
import { checkDomain, decodeEntities, type DomainFilterConfig } from "../web-utils.js";

const DEFAULT_NUM_RESULTS = 8;
const SEARCH_TIMEOUT_MS = 15_000;

const inputSchema = z.object({
  query: z.string().describe("The search query"),
  numResults: z
    .number()
    .int()
    .min(1)
    .max(20)
    .optional()
    .describe("Number of results to return (default 8, max 20)"),
});

export type WebSearchConfig = DomainFilterConfig;

interface SearchResult {
  title: string;
  url: string;
  snippet: string;
}

function filterResults(
  results: SearchResult[],
  config: WebSearchConfig,
): SearchResult[] {
  return results.filter((r) => {
    let hostname: string;
    try {
      hostname = new URL(r.url).hostname;
    } catch {
      return false;
    }
    return checkDomain(hostname, config, "web_search") === null;
  });
}

/**
 * Search via DuckDuckGo HTML — no API key needed.
 * Parses the lite HTML results page for links and snippets.
 */
async function ddgSearch(query: string, numResults: number): Promise<SearchResult[]> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), SEARCH_TIMEOUT_MS);

  try {
    const params = new URLSearchParams({ q: query, kl: "" });
    const res = await fetch(`https://html.duckduckgo.com/html/?${params.toString()}`, {
      headers: {
        "User-Agent": "moneypenny/1.0",
        Accept: "text/html",
      },
      signal: controller.signal,
    });
    if (!res.ok) {
      throw new Error(`DuckDuckGo returned HTTP ${String(res.status)}`);
    }
    const html = await res.text();
    return parseResults(html, numResults);
  } finally {
    clearTimeout(timer);
  }
}

function parseResults(html: string, limit: number): SearchResult[] {
  const results: SearchResult[] = [];
  const resultBlocks = html.split(/class="result\b/);

  for (let i = 1; i < resultBlocks.length && results.length < limit; i++) {
    const block = resultBlocks[i]!;

    const linkMatch = block.match(
      /class="result__a"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)<\/a>/,
    );
    if (!linkMatch) continue;

    let rawUrl = linkMatch[1]!;
    // DuckDuckGo wraps URLs in a redirect; extract the actual target
    const uddg = rawUrl.match(/[?&]uddg=([^&]+)/);
    if (uddg) rawUrl = decodeURIComponent(uddg[1]!);

    const title = linkMatch[2]!.replace(/<[^>]+>/g, "").trim();
    if (!title) continue;

    const snippetMatch = block.match(
      /class="result__snippet"[^>]*>([\s\S]*?)<\/(?:a|td|div|span)/,
    );
    const snippet = snippetMatch
      ? decodeEntities(snippetMatch[1]!.replace(/<[^>]+>/g, "")).trim()
      : "";

    results.push({ title, url: rawUrl, snippet });
  }

  return results;
}

export function createWebSearchTool(config: WebSearchConfig = {}): ToolDefinition {
  return {
    name: "web_search",
    description:
      "Search the web and return a list of results with titles, URLs, and snippets. " +
      "Useful for finding documentation, looking up error messages, or discovering solutions.",
    inputSchema,
    async execute(input): Promise<string> {
      try {
        const { query, numResults } = input as z.infer<typeof inputSchema>;
        const limit = numResults ?? DEFAULT_NUM_RESULTS;

        const raw = await ddgSearch(query, limit + 5); // over-fetch to survive filtering
        const results = filterResults(raw, config).slice(0, limit);

        if (results.length === 0) {
          return "No results found.";
        }

        const formatted = results
          .map(
            (r, i) =>
              `${String(i + 1)}. ${r.title}\n   ${r.url}${r.snippet ? `\n   ${r.snippet}` : ""}`,
          )
          .join("\n\n");

        return truncate(`Search results for: ${query}\n\n${formatted}`);
      } catch (e) {
        if (e instanceof DOMException && e.name === "AbortError") {
          return "Error: search request timed out.";
        }
        return `Error: ${e instanceof Error ? e.message : String(e)}`;
      }
    },
  };
}

export const webSearchTool: ToolDefinition = createWebSearchTool();
