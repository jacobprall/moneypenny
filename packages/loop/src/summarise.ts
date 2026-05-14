import type { LocalGen } from "@swe/db";
import { createProvider, type ProviderName } from "./provider.js";

export interface SummariseConfig {
  model: string;
  provider: ProviderName;
  apiKey: string;
  localGen?: LocalGen;
}

export interface MessagePair {
  userText: string;
  assistantText: string;
}

const SYSTEM_PROMPT =
  "You are a session labeller. Reply with ONLY a short title (max 60 chars, no quotes, no punctuation at the end) that describes what the conversation is about. Do not explain your answer.";

function buildLocalPrompt(pair: MessagePair): string {
  return `<|im_start|>system
${SYSTEM_PROMPT}<|im_end|>
<|im_start|>user
User said: ${pair.userText}
Assistant replied: ${pair.assistantText}<|im_end|>
<|im_start|>assistant
`;
}

function cleanLabel(raw: string): string | null {
  let label = raw.trim().replace(/^["']|["']$/g, "").trim();
  label = label.split("\n")[0]?.trim() ?? "";
  if (label.length === 0) return null;
  return label.slice(0, 60);
}

/**
 * Generate a short session label. Prefers local gen (zero-cost, instant)
 * and falls back to cloud LLM if local gen is unavailable.
 * Returns null on any error so callers can skip silently.
 */
export async function summariseSession(
  pair: MessagePair,
  config: SummariseConfig,
): Promise<string | null> {
  if (config.localGen?.isAvailable()) {
    try {
      const raw = config.localGen.generate(buildLocalPrompt(pair), { maxTokens: 20 });
      return cleanLabel(raw);
    } catch { /* fall through to cloud */ }
  }

  try {
    const provider = await createProvider(config.provider, config.apiKey);
    const gen = provider.stream({
      model: config.model,
      system: [{ type: "text", text: SYSTEM_PROMPT }],
      messages: [
        {
          role: "user",
          content: `User said: ${pair.userText}\nAssistant replied: ${pair.assistantText}`,
        },
      ],
      tools: [],
      maxTokens: 80,
    });

    let label = "";
    for await (const event of gen) {
      if (event.type === "text_delta") {
        label += event.text;
      }
    }

    return cleanLabel(label);
  } catch {
    return null;
  }
}
