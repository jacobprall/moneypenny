import type { ContentBlock, AnthropicMessage, AnthropicToolDef } from "@moneypenny/ctx";
import type { AssistantMessage, TokenUsage } from "./types.js";

export type ProviderName = "anthropic" | "openai" | "google";

export type StreamEvent =
  | { type: "text_delta"; text: string }
  | { type: "complete"; message: AssistantMessage; usage: TokenUsage };

export interface CompletionParams {
  model: string;
  system: ContentBlock[];
  messages: AnthropicMessage[];
  tools: AnthropicToolDef[];
  maxTokens?: number;
  signal?: AbortSignal;
}

export interface LLMProvider {
  readonly name: ProviderName;
  stream(params: CompletionParams): AsyncGenerator<StreamEvent>;
}

export async function createProvider(provider: ProviderName, apiKey: string): Promise<LLMProvider> {
  switch (provider) {
    case "anthropic": {
      const { createAnthropicProvider } = await import("./anthropic.js");
      return createAnthropicProvider(apiKey);
    }
    case "openai": {
      const { createOpenAIProvider } = await import("./openai.js");
      return createOpenAIProvider(apiKey);
    }
    case "google": {
      const { createGoogleProvider } = await import("./google.js");
      return createGoogleProvider(apiKey);
    }
  }
}

const MODEL_PREFIXES: [string, ProviderName][] = [
  ["claude-", "anthropic"],
  ["gpt-", "openai"],
  ["o1", "openai"],
  ["o3", "openai"],
  ["o4", "openai"],
  ["chatgpt-", "openai"],
  ["gemini-", "google"],
];

export function inferProvider(model: string): ProviderName {
  for (const [prefix, provider] of MODEL_PREFIXES) {
    if (model.startsWith(prefix)) return provider;
  }
  console.warn(`[agent-loop] Could not infer provider for model "${model}", defaulting to "anthropic"`);
  return "anthropic";
}
