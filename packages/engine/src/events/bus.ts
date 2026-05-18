import type { Database } from "bun:sqlite";
import type { Event, EventInput } from "./types.js";

const TOKEN_TYPE = "message.assistant.token" as const;
const MAX_QUEUE = 1000;

type Sub = {
  filter?: { sessionId?: string; types?: string[] };
  enqueue: (e: Event) => void;
};

function matchesFilter(
  e: Event,
  filter?: { sessionId?: string; types?: string[] },
): boolean {
  if (filter?.sessionId !== undefined && e.session_id !== filter.sessionId) {
    return false;
  }
  if (filter?.types !== undefined && !filter.types.includes(e.type)) {
    return false;
  }
  return true;
}

export class EventBus {
  private stmt: ReturnType<Database["prepare"]>;
  private subs = new Set<Sub>();
  private ephemeralId = -1;

  constructor(private readonly writeDb: Database) {
    this.stmt = writeDb.prepare(
      `INSERT INTO events (type, session_id, run_id, blueprint, detail, created_at)
       VALUES (?, ?, ?, ?, ?, unixepoch())`,
    );
  }

  emit(input: EventInput): void {
    const created_at = Math.floor(Date.now() / 1000);
    let event: Event;
    if (input.type === TOKEN_TYPE) {
      event = {
        ...input,
        id: this.ephemeralId--,
        created_at,
      };
    } else {
      this.stmt.run(
        input.type,
        input.session_id ?? null,
        input.run_id ?? null,
        input.blueprint ?? null,
        input.detail !== undefined ? JSON.stringify(input.detail) : null,
      );
      event = {
        ...input,
        id: Number(this.writeDb.lastInsertRowid),
        created_at,
      };
    }
    for (const sub of this.subs) {
      if (!matchesFilter(event, sub.filter)) continue;
      sub.enqueue(event);
    }
  }

  subscribe(
    filter?: { sessionId?: string; types?: string[] },
  ): AsyncIterable<Event> & { close: () => void } {
    const queue: Event[] = [];
    let resume: (() => void) | undefined;
    let closed = false;

    const enqueue = (e: Event): void => {
      if (closed) return;
      if (queue.length >= MAX_QUEUE) queue.shift();
      queue.push(e);
      resume?.();
      resume = undefined;
    };

    const sub: Sub = {
      filter,
      enqueue,
    };
    this.subs.add(sub);

    const tryResume = (): void => {
      if (resume && (queue.length > 0 || closed)) {
        const r = resume;
        resume = undefined;
        r();
      }
    };

    return {
      close: () => {
        closed = true;
        this.subs.delete(sub);
        tryResume();
      },
      async *[Symbol.asyncIterator](): AsyncGenerator<Event> {
        try {
          while (!closed) {
            if (queue.length === 0) {
              await new Promise<void>((r) => {
                resume = r;
              });
            }
            if (closed) break;
            while (queue.length > 0 && !closed) {
              yield queue.shift()!;
            }
          }
        } finally {
          closed = true;
          this.subs.delete(sub);
          tryResume();
        }
      },
    };
  }
}
