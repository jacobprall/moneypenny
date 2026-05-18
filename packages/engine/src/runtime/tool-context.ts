let toolCallingSessionId: string | undefined;
let toolCallingRunId: string | undefined;

export function setToolCallingSession(id: string | undefined): void {
  toolCallingSessionId = id;
}

export function getToolCallingSession(): string | undefined {
  return toolCallingSessionId;
}

export function setToolCallingRunId(id: string | undefined): void {
  toolCallingRunId = id;
}

export function getToolCallingRunId(): string | undefined {
  return toolCallingRunId;
}
