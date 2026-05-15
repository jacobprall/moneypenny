import type { Interface as ReadlineInterface } from "node:readline";
import { accent, muted } from "./display.js";

export interface SelectOption<T = string> {
  label: string;
  value: T;
  hint?: string;
}

/**
 * Render an interactive list the user navigates with arrow keys / j-k
 * and confirms with Enter.  Returns `null` on Escape or Ctrl-C.
 *
 * Temporarily switches stdin to raw mode and pauses any active readline
 * interface so keystrokes are handled here instead of by the line editor.
 */
export async function interactiveSelect<T>(
  options: SelectOption<T>[],
  opts?: {
    initialIndex?: number;
    rl?: ReadlineInterface;
  },
): Promise<T | null> {
  if (options.length === 0) return null;
  if (!process.stdin.isTTY) return null;

  const { stdin, stdout } = process;
  const rl = opts?.rl;
  rl?.pause();

  let cursor = Math.max(0, Math.min(opts?.initialIndex ?? 0, options.length - 1));

  stdout.write("\x1b[?25l"); // hide cursor

  function render(first: boolean): void {
    if (!first) {
      stdout.write(`\x1b[${String(options.length)}A`);
    }
    for (let i = 0; i < options.length; i++) {
      stdout.write("\x1b[2K");
      const active = i === cursor;
      const marker = active ? accent("❯") : " ";
      const label = active ? accent(options[i]!.label) : options[i]!.label;
      const hint = options[i]!.hint ? ` ${muted(options[i]!.hint!)}` : "";
      stdout.write(`    ${marker} ${label}${hint}\n`);
    }
  }

  render(true);

  return new Promise<T | null>((resolve) => {
    const wasRaw = stdin.isRaw;
    stdin.setRawMode(true);
    stdin.resume();
    stdin.setEncoding("utf8");

    function cleanup(value: T | null): void {
      stdin.removeListener("data", onData);
      stdin.setRawMode(wasRaw ?? false);
      stdout.write("\x1b[?25h"); // show cursor
      stdout.write("\n");
      rl?.resume();
      resolve(value);
    }

    function onData(key: string): void {
      if (key === "\x1b[A" || key === "k") {
        cursor = cursor > 0 ? cursor - 1 : options.length - 1;
        render(false);
        return;
      }
      if (key === "\x1b[B" || key === "j") {
        cursor = cursor < options.length - 1 ? cursor + 1 : 0;
        render(false);
        return;
      }
      if (key === "\r" || key === "\n") {
        cleanup(options[cursor]!.value);
        return;
      }
      if (key === "\x03" || key === "\x1b") {
        cleanup(null);
        return;
      }
    }

    stdin.on("data", onData);
  });
}
