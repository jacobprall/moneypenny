export type BlueprintStrategy = "autonomous" | "hitl" | "review";
export type BlueprintTrigger = "manual" | "session_close" | "schedule" | "file_change";
export type BlueprintSource = "global" | "repo";

export interface BlueprintPermissions {
  filesystem: "read" | "readwrite";
  network: boolean;
  shell: boolean;
}

export interface BlueprintContext {
  conventions: boolean;
  skills: string[];
}

export interface Blueprint {
  name: string;
  model?: string;
  tools: string[] | null;
  permissions: BlueprintPermissions;
  strategy: BlueprintStrategy;
  pause_after: string[];
  max_turns: number;
  context: BlueprintContext;
  trigger_on: BlueprintTrigger;
  schedule?: string;
  file_glob?: string[];
  body: string;
  path: string;
  source: BlueprintSource;
}
