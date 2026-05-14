import { createProvider, type ProviderName } from "./provider.js";

export interface SummariseConfig {
  model: string;
  provider: ProviderName;
  apiKey: string;
}

export interface MessagePair {
  userText: string;
  assistantText: string;
}

const SYSTEM_PROMPT =
  "You are a session labeller. Reply with ONLY a short title (max 60 chars, no quotes, no punctuation at the end) that describes what the conversation is about. Do not explain your answer.";

/**
 * Calls the LLM once (non-streaming) and returns a short label string.
 * Returns null on any error so callers can skip silently.
 */
export async function summariseSession(
  pair: MessagePair,
  config: SummariseConfig,
): Promise<string | null> {
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

    label = label.trim().replace(/^["']|["']$/g, "").trim();
    if (label.length === 0) return null;
    return label.slice(0, 60);
  } catch {
    return null;
  }
}
