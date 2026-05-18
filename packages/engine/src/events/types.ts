export type EventType =
  | "session.created"
  | "session.status_changed"
  | "session.config_changed"
  | "session.completed"
  | "session.failed"
  | "session.archived"
  | "session.deleted"
  | "run.started"
  | "run.completed"
  | "run.failed"
  | "run.aborted"
  | "message.user"
  | "message.assistant.started"
  | "message.assistant.token"
  | "message.assistant.completed"
  | "message.tool.result"
  | "tool.started"
  | "tool.completed"
  | "tool.failed"
  | "child.spawned"
  | "child.completed"
  | "child.failed"
  | "hitl.checkpoint"
  | "hitl.requested"
  | "hitl.resumed"
  | "knowledge.skill_extracted"
  | "knowledge.convention_detected"
  | "knowledge.pointer_created"
  | "blueprint.loaded"
  | "blueprint.invalid"
  | "blueprint.removed"
  | "schedule.fired"
  | "schedule.failed"
  | "schedule.skipped"
  | "cwd.missing"
  | "permission.denied"
  | "policy.warned"
  | "policy.blocked"
  | "budget.warned"
  | "budget.exceeded"
  | "system.started"
  | "system.shutdown"
  | "index.completed"
  | "tab.opened"
  | "tab.closed";

export type Event = {
  id: number;
  type: EventType;
  session_id?: string;
  run_id?: string;
  blueprint?: string;
  detail?: unknown;
  created_at: number;
};

export type EventInput = Omit<Event, "id" | "created_at">;
