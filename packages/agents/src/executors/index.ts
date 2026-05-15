import type { JobExecutor } from "./types.js";
import { AgentRunExecutor } from "./agent-run.js";
import { PipelineExecutor } from "./pipeline.js";
import { IndexExecutor } from "./index-run.js";
import { SyncExecutor } from "./sync-run.js";
import { CustomExecutor } from "./custom-run.js";

export * from "./types.js";
export { AgentRunExecutor } from "./agent-run.js";
export { PipelineExecutor, isBlockedHttpUrl, type PipelineAction, type PipelineStep } from "./pipeline.js";
export { IndexExecutor } from "./index-run.js";
export { SyncExecutor } from "./sync-run.js";
export {
  CustomExecutor,
  registerCustomJobHandler,
  getCustomJobHandlers,
  type CustomJobHandler,
} from "./custom-run.js";

export function createExecutorRegistry(): Map<string, JobExecutor> {
  const builtIns: JobExecutor[] = [
    new AgentRunExecutor(),
    new PipelineExecutor(),
    new IndexExecutor(),
    new SyncExecutor(),
    new CustomExecutor(),
  ];
  const m = new Map<string, JobExecutor>();
  for (const ex of builtIns) {
    m.set(ex.operation, ex);
  }
  return m;
}
