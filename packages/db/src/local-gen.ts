/**
 * Local text generation via sqlite-ai.
 *
 * Opens a dedicated in-memory SQLite connection, loads the sqlite-ai extension,
 * and exposes a synchronous `generate()` method backed by llm_text_generate().
 * Completely independent of the workspace DB embedding model.
 */

import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import * as path from "node:path";

export interface LocalGenOptions {
  modelPath: string;
  contextSize?: number;
  maxPredict?: number;
  gpuLayers?: number;
}

export interface LocalGen {
  generate(prompt: string, opts?: { maxTokens?: number }): string;
  isAvailable(): boolean;
  close(): void;
}

const DEFAULT_CONTEXT_SIZE = 2048;
const DEFAULT_MAX_PREDICT = 256;
const DEFAULT_GPU_LAYERS = 99;

const TEXTGEN_MODEL_FILE = "qwen2.5-0.5b-instruct-q4_k_m.gguf";

export function defaultTextGenModelPath(): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? "~";
  return path.join(home, ".swe", "models", TEXTGEN_MODEL_FILE);
}

function tryLoadAIExtension(database: Database): boolean {
  try {
    const { getExtensionPath } = require("@sqliteai/sqlite-ai") as { getExtensionPath: () => string };
    database.loadExtension(getExtensionPath());
    return true;
  } catch {
    return false;
  }
}

/**
 * Create a local text generation instance. Returns a stub with
 * isAvailable()=false if the model or sqlite-ai extension is missing.
 */
export function createLocalGen(options?: Partial<LocalGenOptions>): LocalGen {
  const modelPath = options?.modelPath ?? defaultTextGenModelPath();

  if (!existsSync(modelPath)) {
    return {
      generate() { throw new Error("Local gen model not available"); },
      isAvailable() { return false; },
      close() {},
    };
  }

  let db: Database | null = null;
  let ready = false;

  try {
    db = new Database(":memory:");
    if (!tryLoadAIExtension(db)) {
      db.close();
      return {
        generate() { throw new Error("sqlite-ai extension not available"); },
        isAvailable() { return false; },
        close() {},
      };
    }

    const ctxSize = options?.contextSize ?? DEFAULT_CONTEXT_SIZE;
    const maxPredict = options?.maxPredict ?? DEFAULT_MAX_PREDICT;
    const gpuLayers = options?.gpuLayers ?? DEFAULT_GPU_LAYERS;

    db.prepare(`SELECT llm_model_load(?, ?)`).get(modelPath, `gpu_layers=${gpuLayers}`);
    db.prepare(`SELECT llm_context_create_textgen(?)`).get(`context_size=${ctxSize},n_predict=${maxPredict}`);
    db.prepare(`SELECT llm_sampler_init_greedy()`).get();

    ready = true;
  } catch {
    if (db) try { db.close(); } catch { /* ignore */ }
    return {
      generate() { throw new Error("Failed to initialize local gen"); },
      isAvailable() { return false; },
      close() {},
    };
  }

  const activeDb = db;

  return {
    generate(prompt: string, opts?: { maxTokens?: number }): string {
      if (!ready || !activeDb) throw new Error("Local gen not initialized");
      const nPredict = opts?.maxTokens ?? DEFAULT_MAX_PREDICT;
      const row = activeDb.prepare(`SELECT llm_text_generate(?, ?) AS result`).get(prompt, nPredict) as { result: string } | undefined;
      return row?.result?.trim() ?? "";
    },

    isAvailable(): boolean {
      return ready;
    },

    close(): void {
      if (!activeDb) return;
      try {
        activeDb.prepare(`SELECT llm_context_free()`).get();
        activeDb.prepare(`SELECT llm_model_free()`).get();
      } catch { /* best effort */ }
      try { activeDb.close(); } catch { /* ignore */ }
      ready = false;
    },
  };
}
