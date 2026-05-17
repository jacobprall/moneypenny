import { anthropic } from "@ai-sdk/anthropic";
import { openai } from "@ai-sdk/openai";
import { google } from "@ai-sdk/google";
import { createOpenAI } from "@ai-sdk/openai";
import { generateText } from "ai";

export type ModelTier = "strong" | "fast" | "local";

export interface ModelConfig {
  strong: string;
  fast: string;
  local?: string;
  ollamaBaseUrl?: string;
}

const DEFAULT_CONFIG: ModelConfig = {
  strong: "claude-sonnet-4-20250514",
  fast: "claude-sonnet-4-20250514",
  local: undefined,
  ollamaBaseUrl: "http://localhost:11434/v1",
};

let _config: ModelConfig = { ...DEFAULT_CONFIG };

export function configureLlm(config: Partial<ModelConfig>): void {
  _config = { ...DEFAULT_CONFIG, ...config };
}

export function getLlmConfig(): ModelConfig {
  return { ..._config };
}

export function resolveModel(modelStr: string) {
  if (modelStr.startsWith("ollama:")) {
    const modelName = modelStr.slice("ollama:".length);
    const ollama = createOpenAI({
      baseURL: _config.ollamaBaseUrl ?? "http://localhost:11434/v1",
      apiKey: "ollama",
    });
    return ollama(modelName);
  }

  if (modelStr.startsWith("openai:")) {
    return openai(modelStr.slice("openai:".length));
  }
  if (modelStr.startsWith("google:")) {
    return google(modelStr.slice("google:".length));
  }
  if (modelStr.startsWith("anthropic:")) {
    return anthropic(modelStr.slice("anthropic:".length));
  }

  if (modelStr.startsWith("claude")) return anthropic(modelStr);
  if (
    modelStr.startsWith("gpt") ||
    modelStr.startsWith("o1") ||
    modelStr.startsWith("o3") ||
    modelStr.startsWith("o4")
  )
    return openai(modelStr);
  if (modelStr.startsWith("gemini")) return google(modelStr);

  return anthropic(modelStr);
}

export function modelForTier(tier: ModelTier): string {
  switch (tier) {
    case "strong":
      return _config.strong;
    case "fast":
      return _config.fast;
    case "local":
      return _config.local ?? _config.fast;
  }
}

export async function llm(
  tier: ModelTier,
  prompt: string,
  opts?: { maxTokens?: number; model?: string },
): Promise<string> {
  const modelStr = opts?.model ?? modelForTier(tier);
  const model = resolveModel(modelStr);

  const { text } = await generateText({
    model,
    prompt,
    maxTokens: opts?.maxTokens,
  });

  return text;
}

export async function llmJson<T = unknown>(
  tier: ModelTier,
  prompt: string,
  opts?: { model?: string },
): Promise<T | null> {
  const text = await llm(tier, prompt, opts);
  try {
    const match = text.match(/[\[{][\s\S]*[\]}]/);
    if (!match) return null;
    return JSON.parse(match[0]) as T;
  } catch {
    return null;
  }
}
