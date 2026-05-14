import { definePrompt, type AnthropicToolDef, type Prompt } from "@moneypenny/ctx";
import { getConfig, getConversation, listSkillCatalog } from "@moneypenny/db";

const DEFAULT_SYSTEM_INSTRUCTIONS = [
  "You are mp, an expert AI coding assistant running locally against the user's repository.",
  "",
  "- When exploring or answering questions about the codebase, **search first** — use code_search before reading individual files. It returns the most relevant snippets across the whole repo in one call.",
  "- Use file_read when you already know the exact file and line range you need, or to read a file discovered via search.",
  "- Use tools proactively: search code, read files, inspect git state, run safe shell commands when needed.",
  "- Prefer concise, accurate answers grounded in repo evidence.",
  "- When editing files, minimize churn and preserve existing style.",
  "- Explain your reasoning briefly when it helps the user.",
  "",
  `Repository root on disk will be injected into tool contexts (working directory defaults to repo).`,
].join("\n");

export function createDefaultPrompt(tools: AnthropicToolDef[]): Prompt {
  return definePrompt({
    sections: [
      {
        name: "system",
        placement: "static",
        resolve: (db, _ctx) => {
          const custom = getConfig(db, "system_instructions");
          return custom ?? DEFAULT_SYSTEM_INSTRUCTIONS;
        },
      },
      {
        name: "skill-catalog",
        placement: "dynamic",
        resolve: (db) => {
          const catalog = listSkillCatalog(db);
          if (catalog.length === 0) return "";
          const learned = catalog.filter((e) => e.source === "learned");
          const other = catalog.filter((e) => e.source !== "learned");
          const lines = [
            "## Available Skills",
            "",
            "Use the read_skill tool to load a skill's full instructions when relevant.",
            "",
          ];
          for (const entry of other) {
            lines.push(`- **${entry.name}**: ${entry.description}`);
          }
          if (learned.length > 0) {
            lines.push("");
            lines.push("### Learned from previous sessions");
            lines.push("");
            lines.push("These skills were automatically extracted from past conversations. Load them when the topic is relevant.");
            lines.push("");
            for (const entry of learned) {
              lines.push(`- **${entry.name}**: ${entry.description}`);
            }
          }
          return lines.join("\n");
        },
      },
    ],
    conversationResolver: (db, _ctx) => getConversation(db),
    tools,
  });
}
