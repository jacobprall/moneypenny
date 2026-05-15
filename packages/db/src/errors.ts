export function sqlError(op: string, cause: unknown): Error {
  const msg = cause instanceof Error ? cause.message : String(cause);
  return new Error(`${op} failed: ${msg}`);
}

export class NotIndexedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NotIndexedError";
  }
}

export function isHarmlessIndexError(err: unknown): boolean {
  if (err instanceof NotIndexedError) return true;
  const msg = err instanceof Error ? err.message : String(err);
  return /no such table|no such column|database.*not/.test(msg);
}
