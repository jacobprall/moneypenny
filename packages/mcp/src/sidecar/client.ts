export type PolicyEffect = "allow" | "deny" | "audit" | "confirm";

export interface PolicyEvaluateResult {
  effect: PolicyEffect;
  matchedPolicy: { id: string; name: string } | null;
  reason: string;
}

export interface SearchHit {
  path: string;
  startLine: number;
  endLine: number;
  score: number;
  chunkText: string;
}

export class SidecarClient {
  constructor(
    private readonly baseUrl: string,
    private readonly fetchFn: typeof fetch = fetch,
  ) {}

  private url(path: string): string {
    const base = this.baseUrl.replace(/\/$/, "");
    return `${base}${path.startsWith("/") ? path : `/${path}`}`;
  }

  async health(): Promise<{ status: string } | null> {
    try {
      const res = await this.fetchFn(this.url("/health"), { method: "GET" });
      if (!res.ok) return null;
      return (await res.json()) as { status: string };
    } catch {
      return null;
    }
  }

  async codeSearch(
    query: string,
    opts?: { limit?: number; languages?: string[]; paths?: string[] },
  ): Promise<SearchHit[]> {
    try {
      const res = await this.fetchFn(this.url("/api/search"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          query,
          limit: opts?.limit,
          languages: opts?.languages,
          paths: opts?.paths,
        }),
      });
      if (!res.ok) return [];
      const data = (await res.json()) as { results?: SearchHit[] };
      return data.results ?? [];
    } catch {
      return [];
    }
  }

  async evaluatePolicy(input: {
    actor: string;
    action: string;
    resource: string;
    denyByDefault?: boolean;
    sessionId?: string;
  }): Promise<PolicyEvaluateResult | null> {
    try {
      const res = await this.fetchFn(this.url("/api/policy/evaluate"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(input),
      });
      if (!res.ok) return null;
      return (await res.json()) as PolicyEvaluateResult;
    } catch {
      return null;
    }
  }

  async queryAuditLog(opts?: { limit?: number; type?: string; sessionId?: string }): Promise<unknown[]> {
    try {
      const q = new URLSearchParams();
      if (opts?.limit != null) q.set("limit", String(opts.limit));
      if (opts?.type) q.set("type", opts.type);
      if (opts?.sessionId) q.set("sessionId", opts.sessionId);
      const res = await this.fetchFn(this.url(`/api/events?${q.toString()}`), { method: "GET" });
      if (!res.ok) return [];
      const data = (await res.json()) as { events?: unknown[] };
      return data.events ?? [];
    } catch {
      return [];
    }
  }
}
