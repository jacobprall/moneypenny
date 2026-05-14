import { z } from "zod";
import type { ToolDefinition } from "../types.js";
import { truncate } from "../utils.js";
import { checkDomain, decodeEntities, type DomainFilterConfig } from "../web-utils.js";

const DEFAULT_TIMEOUT_MS = 15_000;
const MAX_BODY_BYTES = 2 * 1024 * 1024; // 2 MB

const inputSchema = z.object({
  url: z.string().url().describe("The URL to fetch"),
  headers: z
    .record(z.string())
    .optional()
    .describe("Optional HTTP headers to include"),
  timeout: z
    .number()
    .int()
    .positive()
    .optional()
    .describe("Timeout in milliseconds (default 15000)"),
});

export type WebFetchConfig = DomainFilterConfig;

function htmlToText(html: string): string {
  let text = html;
  text = text.replace(/<script[\s\S]*?<\/script>/gi, "");
  text = text.replace(/<style[\s\S]*?<\/style>/gi, "");
  text = text.replace(/<(br|hr)\s*\/?>/gi, "\n");
  text = text.replace(/<\/(p|div|h[1-6]|li|tr|blockquote|section|article)>/gi, "\n\n");
  text = text.replace(/<[^>]+>/g, "");
  text = decodeEntities(text);
  text = text.replace(/[ \t]+/g, " ");
  text = text.replace(/\n{3,}/g, "\n\n");
  return text.trim();
}

export function createWebFetchTool(config: WebFetchConfig = {}): ToolDefinition {
  return {
    name: "web_fetch",
    description:
      "Fetch a URL and return its contents as text. HTML pages are converted to readable text. " +
      "Useful for reading documentation, API responses, or any public web content.",
    inputSchema,
    async execute(input): Promise<string> {
      try {
        const { url, headers, timeout } = input as z.infer<typeof inputSchema>;

        const parsed = new URL(url);
        if (!["http:", "https:"].includes(parsed.protocol)) {
          return `Error: only http/https URLs are supported.`;
        }

        const domainErr = checkDomain(parsed.hostname, config, "web_fetch");
        if (domainErr) return `Error: ${domainErr}`;

        const controller = new AbortController();
        const timer = setTimeout(
          () => controller.abort(),
          timeout ?? DEFAULT_TIMEOUT_MS,
        );

        let res: Response;
        try {
          res = await fetch(url, {
            headers: {
              "User-Agent": "moneypenny/1.0",
              Accept: "text/html, application/json, text/plain, */*",
              ...headers,
            },
            signal: controller.signal,
            redirect: "follow",
          });
        } finally {
          clearTimeout(timer);
        }

        if (!res.ok) {
          return `Error: HTTP ${String(res.status)} ${res.statusText}`;
        }

        const contentType = res.headers.get("content-type") ?? "";
        const contentLength = Number(res.headers.get("content-length") ?? "0");
        if (contentLength > 0 && contentLength > MAX_BODY_BYTES) {
          return `Error: response too large (${String(Math.round(contentLength / 1024))}KB, max ${String(MAX_BODY_BYTES / 1024)}KB).`;
        }

        const buf = await res.arrayBuffer();
        if (buf.byteLength > MAX_BODY_BYTES) {
          return `Error: response body too large (${String(Math.round(buf.byteLength / 1024))}KB).`;
        }
        const raw = new TextDecoder().decode(buf);

        const isHtml = contentType.includes("html") || raw.trimStart().startsWith("<!");
        const body = isHtml ? htmlToText(raw) : raw;

        const meta = [`url: ${url}`, `status: ${String(res.status)}`, `content-type: ${contentType}`].join("\n");
        return truncate(`${meta}\n\n${body}`);
      } catch (e) {
        if (e instanceof DOMException && e.name === "AbortError") {
          return "Error: request timed out.";
        }
        return `Error: ${e instanceof Error ? e.message : String(e)}`;
      }
    },
  };
}

export const webFetchTool: ToolDefinition = createWebFetchTool();
