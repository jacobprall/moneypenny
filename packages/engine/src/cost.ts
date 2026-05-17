interface ModelRates {
  inputPerMTok: number;
  outputPerMTok: number;
  cachedInputPerMTok: number;
}

export interface TokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens?: number;
}

const TABLE: Record<string, ModelRates> = {
  "claude-sonnet-4-6": { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 },
  "claude-sonnet-4-20250514": { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 },
  "claude-opus-4-20250514": { inputPerMTok: 15, outputPerMTok: 75, cachedInputPerMTok: 1.5 },
  "claude-3-5-sonnet-20241022": { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 },
  "claude-3-5-haiku-20241022": { inputPerMTok: 0.8, outputPerMTok: 4, cachedInputPerMTok: 0.08 },
  "claude-3-opus-20240229": { inputPerMTok: 15, outputPerMTok: 75, cachedInputPerMTok: 1.5 },
  "claude-3-haiku-20240307": { inputPerMTok: 0.25, outputPerMTok: 1.25, cachedInputPerMTok: 0.03 },
  "gpt-4o": { inputPerMTok: 2.5, outputPerMTok: 10, cachedInputPerMTok: 1.25 },
  "gpt-4o-mini": { inputPerMTok: 0.15, outputPerMTok: 0.6, cachedInputPerMTok: 0.075 },
  "o3": { inputPerMTok: 2, outputPerMTok: 8, cachedInputPerMTok: 1 },
  "o3-mini": { inputPerMTok: 1.1, outputPerMTok: 4.4, cachedInputPerMTok: 0.55 },
  "o4-mini": { inputPerMTok: 1.1, outputPerMTok: 4.4, cachedInputPerMTok: 0.55 },
  "o1": { inputPerMTok: 15, outputPerMTok: 60, cachedInputPerMTok: 7.5 },
  "gemini-2.5-pro": { inputPerMTok: 1.25, outputPerMTok: 10, cachedInputPerMTok: 0.315 },
  "gemini-2.5-flash": { inputPerMTok: 0.15, outputPerMTok: 0.6, cachedInputPerMTok: 0.0375 },
  "gemini-2.0-flash": { inputPerMTok: 0.1, outputPerMTok: 0.4, cachedInputPerMTok: 0.025 },
  "gemini-1.5-pro": { inputPerMTok: 1.25, outputPerMTok: 5, cachedInputPerMTok: 0.315 },
  "gemini-1.5-flash": { inputPerMTok: 0.075, outputPerMTok: 0.3, cachedInputPerMTok: 0.018 },
};

const PREFIX_RATES: [string, ModelRates][] = [
  ["claude-opus-4", { inputPerMTok: 15, outputPerMTok: 75, cachedInputPerMTok: 1.5 }],
  ["claude-sonnet-4", { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 }],
  ["claude-3-5-sonnet", { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 }],
  ["claude-3-5-haiku", { inputPerMTok: 0.8, outputPerMTok: 4, cachedInputPerMTok: 0.08 }],
  ["claude-3-opus", { inputPerMTok: 15, outputPerMTok: 75, cachedInputPerMTok: 1.5 }],
  ["claude-3-haiku", { inputPerMTok: 0.25, outputPerMTok: 1.25, cachedInputPerMTok: 0.03 }],
  ["gpt-4o-mini", { inputPerMTok: 0.15, outputPerMTok: 0.6, cachedInputPerMTok: 0.075 }],
  ["gpt-4o", { inputPerMTok: 2.5, outputPerMTok: 10, cachedInputPerMTok: 1.25 }],
  ["o4-mini", { inputPerMTok: 1.1, outputPerMTok: 4.4, cachedInputPerMTok: 0.55 }],
  ["o3-mini", { inputPerMTok: 1.1, outputPerMTok: 4.4, cachedInputPerMTok: 0.55 }],
  ["o3", { inputPerMTok: 2, outputPerMTok: 8, cachedInputPerMTok: 1 }],
  ["o1", { inputPerMTok: 15, outputPerMTok: 60, cachedInputPerMTok: 7.5 }],
  ["gemini-2.5-pro", { inputPerMTok: 1.25, outputPerMTok: 10, cachedInputPerMTok: 0.315 }],
  ["gemini-2.5-flash", { inputPerMTok: 0.15, outputPerMTok: 0.6, cachedInputPerMTok: 0.0375 }],
  ["gemini-2.0-flash", { inputPerMTok: 0.1, outputPerMTok: 0.4, cachedInputPerMTok: 0.025 }],
  ["gemini-1.5-pro", { inputPerMTok: 1.25, outputPerMTok: 5, cachedInputPerMTok: 0.315 }],
  ["gemini-1.5-flash", { inputPerMTok: 0.075, outputPerMTok: 0.3, cachedInputPerMTok: 0.018 }],
];

const DEFAULT_RATES: ModelRates = { inputPerMTok: 3, outputPerMTok: 15, cachedInputPerMTok: 0.3 };

function ratesFor(model: string): ModelRates {
  const exact = TABLE[model];
  if (exact) return exact;

  for (const [prefix, rates] of PREFIX_RATES) {
    if (model.startsWith(prefix)) return rates;
  }

  return DEFAULT_RATES;
}

export function calculateCost(
  model: string,
  usage: { promptTokens: number; completionTokens: number; cacheReadTokens?: number },
): number {
  const r = ratesFor(model);
  const inputTokens = Math.max(0, usage.promptTokens);
  const outputTokens = Math.max(0, usage.completionTokens);
  const cached = Math.max(0, usage.cacheReadTokens ?? 0);
  const nonCachedInput = Math.max(0, inputTokens - cached);
  return Math.max(
    0,
    (nonCachedInput * r.inputPerMTok +
      outputTokens * r.outputPerMTok +
      cached * r.cachedInputPerMTok) /
      1_000_000,
  );
}
