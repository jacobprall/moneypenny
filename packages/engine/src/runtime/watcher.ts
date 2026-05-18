import chokidar from "chokidar";
import type { WatcherDeps } from "./types.js";

type ChokidarHandle = ReturnType<typeof chokidar.watch>;

export class Watcher {
  private codeWatch?: ChokidarHandle;
  private configWatch?: ChokidarHandle;

  constructor(private readonly deps: WatcherDeps) {}

  start(repoRoot: string): void {
    void this.codeWatch?.close();
    this.codeWatch = chokidar.watch(repoRoot, {
      ignoreInitial: true,
      ignored: (p: string) =>
        /node_modules|\.git|dist|build|\.next|coverage/.test(p),
    });
    for (const ev of ["add", "change"] as const) {
      this.codeWatch.on(ev, (p: string) => this.deps.codeOnChange(p));
    }
    this.codeWatch.on("unlink", (p: string) => this.deps.codeOnRemove(p));

    void this.configWatch?.close();
    if (this.deps.configDirs.length) {
      this.configWatch = chokidar.watch(this.deps.configDirs, {
        ignoreInitial: false,
        ignored: (p: string) => /node_modules|\.git/.test(p),
      });
      this.configWatch.on("all", (event: string, path: string) => {
        this.deps.configOnChange(path, event);
      });
    }
  }

  async stop(): Promise<void> {
    await this.codeWatch?.close();
    await this.configWatch?.close();
  }
}
