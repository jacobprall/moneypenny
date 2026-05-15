export interface ShutdownHandler {
  name: string;
  priority: number;
  handler: () => Promise<void>;
}

export interface ShutdownManager {
  register(name: string, handler: () => Promise<void>, priority: number): void;
  shutdown(reason: string): Promise<void>;
  isShuttingDown(): boolean;
}

export function createShutdownManager(options?: { timeoutMs?: number }): ShutdownManager {
  const handlers: ShutdownHandler[] = [];
  let shuttingDown = false;
  const totalTimeout = options?.timeoutMs ?? 15_000;

  return {
    register(name, handler, priority) {
      handlers.push({ name, priority, handler });
    },

    isShuttingDown() {
      return shuttingDown;
    },

    async shutdown(reason: string) {
      if (shuttingDown) return;
      shuttingDown = true;
      console.log(`[shutdown] Initiating graceful shutdown: ${reason}`);

      // Sort by priority descending (highest first)
      const sorted = [...handlers].sort((a, b) => b.priority - a.priority);

      const perHandlerTimeout = Math.floor(totalTimeout / Math.max(sorted.length, 1));

      for (const h of sorted) {
        try {
          await Promise.race([
            h.handler(),
            new Promise<never>((_, reject) =>
              setTimeout(() => reject(new Error(`Shutdown handler "${h.name}" timed out`)), perHandlerTimeout),
            ),
          ]);
          console.log(`[shutdown] ${h.name}: done`);
        } catch (e) {
          console.error(`[shutdown] ${h.name}: ${e instanceof Error ? e.message : String(e)}`);
        }
      }

      console.log(`[shutdown] Complete.`);
    },
  };
}
