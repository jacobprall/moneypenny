export const MAX_RETRIES = 3;
export const INITIAL_BACKOFF_MS = 1000;

export async function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error("Aborted"));
      return;
    }
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(new Error("Aborted"));
      },
      { once: true },
    );
  });
}

export async function* withRetry<T>(
  isRetryable: (e: unknown) => boolean,
  signal: AbortSignal | undefined,
  streamFn: () => AsyncGenerator<T>
): AsyncGenerator<T> {
  let lastError: unknown;

  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    if (signal?.aborted) return;

    if (attempt > 0) {
      const backoff = INITIAL_BACKOFF_MS * Math.pow(2, attempt - 1) + Math.random() * 500;
      await sleep(backoff, signal);
    }

    try {
      yield* streamFn();
      return;
    } catch (e) {
      lastError = e;
      if (!isRetryable(e) || attempt === MAX_RETRIES) {
        throw e;
      }
    }
  }

  throw lastError;
}
