import type { LoopEvent } from "@swe/loop";
import {
  muted,
  Spinner,
  printCost,
  printError,
  printInfo,
  printToolComplete,
  printToolError,
  printToolStart,
} from "./display.js";
import { createRenderer } from "./markdown.js";

/**
 * Encapsulates all terminal rendering for agent loop events.
 * One instance per REPL session -- no module-level globals.
 */
export class EventRenderer {
  private spinner = new Spinner();
  private md = createRenderer();

  handle(event: LoopEvent): void {
    switch (event.type) {
      case "turn.started":
        this.md = createRenderer();
        this.spinner.start("Thinking...");
        break;

      case "llm.streaming":
        this.spinner.stop();
        this.md.write(event.delta);
        break;

      case "llm.complete":
        this.spinner.stop();
        this.md.flush();
        process.stdout.write("\n");
        break;

      case "tool.calling":
        this.spinner.stop();
        this.md.flush();
        printToolStart(event.name, event.input);
        break;

      case "tool.complete":
        printToolComplete(event.name, event.output, event.durationMs);
        this.spinner.start("Thinking...");
        break;

      case "tool.error":
        printToolError(event.name, event.error);
        this.spinner.start("Thinking...");
        break;

      case "turn.complete":
        this.spinner.stop();
        this.md.flush();
        printCost({
          model: event.cost.model,
          inputTokens: event.cost.inputTokens,
          outputTokens: event.cost.outputTokens,
          costUsd: event.cost.costUsd,
          turnNumber: event.cost.turnNumber,
        });
        break;

      case "error":
        this.spinner.stop();
        printError(event.error.message);
        break;

      case "paused":
        this.spinner.stop();
        printInfo(muted(`Paused: ${event.reason}`));
        break;
    }
  }

  stop(): void {
    this.spinner.stop();
  }
}
