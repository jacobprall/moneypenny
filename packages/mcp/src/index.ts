export { createMCPServer } from "./server.js";
export type { MCPServerConfig, MCPServerHandle } from "./types.js";
export { McpClientManager } from "./client.js";
export type { McpServerEntry, McpServersConfig } from "./client.js";
export { createSidecarServer } from "./sidecar/index.js";
export { SidecarClient } from "./sidecar/client.js";
export { appendCodeContext, evaluateActionPolicy } from "./sidecar/hooks.js";
export { writeCursorConfig, writeClaudeConfig } from "./setup.js";
