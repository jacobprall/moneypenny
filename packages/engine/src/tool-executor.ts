export interface ToolCallRequest {
  id: string;
  name: string;
  args: Record<string, unknown>;
}

export interface ToolCallResult {
  id: string;
  name: string;
  result: unknown;
  error?: string;
  durationMs: number;
}

export interface ExecutorConfig {
  timeoutMs?: number;
  maxConcurrent?: number;
  onStart?: (call: ToolCallRequest) => void;
  onComplete?: (result: ToolCallResult) => void;
}

export async function executeToolsParallel(
  calls: ToolCallRequest[],
  toolSet: Record<string, { execute?: (args: any) => Promise<any> }>,
  config: ExecutorConfig = {},
): Promise<ToolCallResult[]> {
  const { timeoutMs = 30_000, maxConcurrent = 5, onStart, onComplete } = config;

  const results: ToolCallResult[] = [];
  const semaphore = new Semaphore(maxConcurrent);

  const promises = calls.map(async (call) => {
    await semaphore.acquire();
    try {
      onStart?.(call);
      const start = performance.now();
      const tool = toolSet[call.name];

      if (!tool?.execute) {
        const result: ToolCallResult = {
          id: call.id,
          name: call.name,
          result: null,
          error: `Unknown tool: ${call.name}`,
          durationMs: 0,
        };
        results.push(result);
        onComplete?.(result);
        return;
      }

      try {
        const value = await Promise.race([
          tool.execute(call.args),
          timeout(timeoutMs, `Tool '${call.name}' timed out after ${timeoutMs}ms`),
        ]);
        const result: ToolCallResult = {
          id: call.id,
          name: call.name,
          result: value,
          durationMs: performance.now() - start,
        };
        results.push(result);
        onComplete?.(result);
      } catch (err) {
        const result: ToolCallResult = {
          id: call.id,
          name: call.name,
          result: null,
          error: err instanceof Error ? err.message : String(err),
          durationMs: performance.now() - start,
        };
        results.push(result);
        onComplete?.(result);
      }
    } finally {
      semaphore.release();
    }
  });

  await Promise.all(promises);
  return results;
}

function timeout(ms: number, message: string): Promise<never> {
  return new Promise((_, reject) =>
    setTimeout(() => reject(new Error(message)), ms),
  );
}

class Semaphore {
  private count: number;
  private waiting: Array<() => void> = [];

  constructor(max: number) {
    this.count = max;
  }

  async acquire(): Promise<void> {
    if (this.count > 0) {
      this.count--;
      return;
    }
    return new Promise<void>((resolve) => {
      this.waiting.push(resolve);
    });
  }

  release(): void {
    const next = this.waiting.shift();
    if (next) {
      next();
    } else {
      this.count++;
    }
  }
}
