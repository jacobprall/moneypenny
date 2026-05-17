import type { Database } from "bun:sqlite";

export type StrategyName = "standard" | "research" | "evolution";

export interface StrategyConfig {
  name: StrategyName;
  maxTurns?: number;
  evolutionTarget?: number;
  evolutionMaxIterations?: number;
}

export interface StrategyResult {
  strategy: StrategyName;
  systemSuffix: string;
  maxSteps: number;
  shouldContinue: (turnCount: number, lastResponse: string) => boolean;
  postProcess?: (response: string) => string;
}

const RESEARCH_SUFFIX = `

## Research Mode

You are operating in research mode. For each piece of information you find:
1. State the **FINDING** clearly
2. Cite the **SOURCE** (file path, function name, or session key)
3. Note any **GAPS** in your understanding

When you have enough information, write a **RESEARCH_COMPLETE** report with:
- Summary of findings
- Key decisions or patterns discovered
- Recommended next steps
`;

const EVOLUTION_SUFFIX = `

## Evolution Mode

You are iteratively improving your response. Each iteration should:
1. Identify weaknesses in the previous attempt
2. Apply specific improvements
3. Explain what changed and why

Focus on producing the highest quality output possible.
`;

export function resolveStrategy(
  db: Database,
  config: StrategyConfig,
): StrategyResult {
  switch (config.name) {
    case "research":
      return {
        strategy: "research",
        systemSuffix: RESEARCH_SUFFIX,
        maxSteps: 10,
        shouldContinue: (turnCount, lastResponse) => {
          if (turnCount >= (config.maxTurns ?? 15)) return false;
          return !lastResponse.includes("RESEARCH_COMPLETE");
        },
      };

    case "evolution":
      return {
        strategy: "evolution",
        systemSuffix: EVOLUTION_SUFFIX,
        maxSteps: 5,
        shouldContinue: (turnCount) => {
          return turnCount < (config.evolutionMaxIterations ?? 5);
        },
      };

    case "standard":
    default:
      return {
        strategy: "standard",
        systemSuffix: "",
        maxSteps: config.maxTurns ?? 5,
        shouldContinue: () => false,
      };
  }
}
