/**
 * Shared utilities for web_fetch and web_search tools.
 */

/**
 * Test whether a glob-style domain pattern matches a hostname.
 * Supports `*` as a wildcard for one or more domain segments.
 *
 * Examples: `*.example.com` matches `foo.example.com` and `a.b.example.com`.
 */
export function domainMatch(pattern: string, hostname: string): boolean {
  const escaped = pattern
    .replace(/[.+?^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, "[\\w.-]*");
  return new RegExp(`^${escaped}$`, "i").test(hostname);
}

export interface DomainFilterConfig {
  allowlist?: string[];
  blocklist?: string[];
}

/**
 * Check if a hostname passes the blocklist/allowlist.
 * Returns an error message if blocked, null if allowed.
 */
export function checkDomain(
  hostname: string,
  config: DomainFilterConfig,
  toolName: string,
): string | null {
  if (config.blocklist?.some((p) => domainMatch(p, hostname))) {
    return `Domain "${hostname}" is blocked by ${toolName} blocklist.`;
  }
  if (
    config.allowlist &&
    config.allowlist.length > 0 &&
    !config.allowlist.some((p) => domainMatch(p, hostname))
  ) {
    return `Domain "${hostname}" is not in the ${toolName} allowlist.`;
  }
  return null;
}

const ENTITY_MAP: Record<string, string> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#39;": "'",
  "&nbsp;": " ",
};

const ENTITY_RE = /&(?:amp|lt|gt|quot|nbsp|#39|#(\d{1,5})|#x([0-9a-fA-F]{1,5}));/g;

/** Decode common HTML entities + numeric character references. */
export function decodeEntities(text: string): string {
  return text.replace(ENTITY_RE, (match, decimal?: string, hex?: string) => {
    if (decimal) return String.fromCharCode(Number(decimal));
    if (hex) return String.fromCharCode(parseInt(hex, 16));
    return ENTITY_MAP[match] ?? match;
  });
}
