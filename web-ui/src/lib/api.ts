const API_BASE =
  typeof window !== "undefined" ? "" : "http://localhost:8080";

export interface ChatRequest {
  message: string;
  agent?: string;
  session_id?: string;
}

export interface ChatResponse {
  response: string;
  session_id: string;
}

export async function chat(
  body: ChatRequest,
  options?: { signal?: AbortSignal | undefined; apiKey?: string }
): Promise<ChatResponse> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (options?.apiKey) {
    headers["Authorization"] = `Bearer ${options.apiKey}`;
  }
  const res = await fetch(`${API_BASE}/v1/chat`, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: options?.signal,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(res.status === 401 ? "Unauthorized" : text || res.statusText);
  }
  return res.json() as Promise<ChatResponse>;
}

export function health(): Promise<{ status: string; version?: string }> {
  return fetch(`${API_BASE}/health`).then((r) => r.json());
}
