import { homedir } from "node:os";
import { isAbsolute, join } from "node:path";
import type { Database } from "bun:sqlite";
import { anthropic } from "@ai-sdk/anthropic";
import { openai, createOpenAI } from "@ai-sdk/openai";
import { google } from "@ai-sdk/google";
import { generateText, type LanguageModel } from "ai";

export type ModelTier = "strong" | "fast" | "local";

export interface SqliteAiConfig {
  modelsDir?: string;       // bare model names resolve under this dir
  contextSize?: number;     // n_ctx
  nPredict?: number;        // default max output tokens
  nThreads?: number;        // 0 = auto
  gpuLayers?: number;       // 99 = all on GPU
}

export interface ModelConfig {
  strong: string;
  fast: string;
  local?: string;
  ollamaBaseUrl?: string;
  sqliteAi?: SqliteAiConfig;
}

const DEFAULT_OLLAMA_URL = "http://localhost:11434/v1";
const DEFAULT_MODELS_DIR = join(
  process.env.MP_DATA ?? join(homedir(), ".moneypenny"),
  "models",
);

const DEFAULT_CONFIG: ModelConfig = {
  strong: "claude-sonnet-4-20250514",
  fast: "claude-sonnet-4-20250514",
  local: undefined,
  ollamaBaseUrl: DEFAULT_OLLAMA_URL,
  sqliteAi: {
    modelsDir: DEFAULT_MODELS_DIR,
    contextSize: 4096,
    nPredict: 1024,
    nThreads: 0,
    gpuLayers: 99,
  },
};

let _config: ModelConfig = { ...DEFAULT_CONFIG };
let _aiDb: Database | null = null;

interface SqliteAiState { db: Database; modelPath: string }
let _sqliteAiState: SqliteAiState | null = null;

// ── Public configuration ──────────────────────────────────────────────────

export function configureLlm(config: Partial<ModelConfig>): void {
  _config = {
    ..._config,
    ...config,
    sqliteAi: { ...DEFAULT_CONFIG.sqliteAi, ..._config.sqliteAi, ...config.sqliteAi },
  };
}

export function getLlmConfig(): ModelConfig {
  return JSON.parse(JSON.stringify(_config));
}

/** Inject the dedicated SQLite connection used for sqlite-ai inference. */
export function setLlmDatabase(db: Database): void {
  if (_aiDb !== db) _sqliteAiState = null;
  _aiDb = db;
}

export function modelForTier(tier: ModelTier): string {
  if (tier === "strong") return _config.strong;
  if (tier === "fast") return _config.fast;
  return _config.local ?? _config.fast;
}

// ── Provider resolution ───────────────────────────────────────────────────

function isSqliteAi(modelStr: string): boolean {
  return modelStr.startsWith("sqliteai:");
}

/** AI SDK resolution. Throws on `sqliteai:` — those go through the SQL path. */
export function resolveModel(modelStr: string): LanguageModel {
  if (isSqliteAi(modelStr)) {
    throw new Error(
      "sqliteai: models cannot be used with the AI SDK; use llm()/llmJson() for local generation",
    );
  }

  if (modelStr.startsWith("ollama:")) {
    const ollama = createOpenAI({
      baseURL: _config.ollamaBaseUrl ?? DEFAULT_OLLAMA_URL,
      apiKey: "ollama",
    });
    return ollama(modelStr.slice("ollama:".length));
  }
  if (modelStr.startsWith("openai:"))    return openai(modelStr.slice("openai:".length));
  if (modelStr.startsWith("google:"))    return google(modelStr.slice("google:".length));
  if (modelStr.startsWith("anthropic:")) return anthropic(modelStr.slice("anthropic:".length));

  if (modelStr.startsWith("claude")) return anthropic(modelStr);
  if (/^(gpt|o[134])/.test(modelStr)) return openai(modelStr);
  if (modelStr.startsWith("gemini")) return google(modelStr);

  return anthropic(modelStr);
}

// ── sqlite-ai provider ────────────────────────────────────────────────────

function resolveSqliteAiPath(modelToken: string): string {
  if (isAbsolute(modelToken)) return modelToken;
  const dir = _config.sqliteAi?.modelsDir ?? DEFAULT_MODELS_DIR;
  return join(dir, modelToken);
}

function ensureSqliteAiLoaded(modelPath: string): Database {
  if (!_aiDb) {
    throw new Error(
      "sqliteai requires a Database; call setLlmDatabase(db) at startup",
    );
  }

  if (_sqliteAiState && _sqliteAiState.db === _aiDb && _sqliteAiState.modelPath === modelPath) {
    return _aiDb;
  }

  if (_sqliteAiState) {
    for (const sql of [
      "SELECT llm_chat_free()",
      "SELECT llm_context_free()",
      "SELECT llm_model_free()",
    ]) {
      try { _aiDb.exec(sql); } catch {}
    }
  }

  const cfg = _config.sqliteAi ?? {};
  const loadOpts = `gpu_layers=${cfg.gpuLayers ?? 99},use_mmap=1`;
  const ctxOpts = [
    `n_ctx=${cfg.contextSize ?? 4096}`,
    `n_predict=${cfg.nPredict ?? 1024}`,
    `n_threads=${cfg.nThreads ?? 0}`,
  ].join(",");

  _aiDb.query("SELECT llm_model_load(?, ?)").get(modelPath, loadOpts);
  _aiDb.query("SELECT llm_context_create_textgen(?)").get(ctxOpts);

  _sqliteAiState = { db: _aiDb, modelPath };
  return _aiDb;
}

async function generateSqliteAi(
  modelToken: string,
  prompt: string,
  opts?: { maxTokens?: number },
): Promise<string> {
  const modelPath = resolveSqliteAiPath(modelToken);
  const db = ensureSqliteAiLoaded(modelPath);

  const generateOpts = opts?.maxTokens ? `n_predict=${opts.maxTokens}` : "";
  const row = db
    .query<{ result: string }, [string, string]>(
      "SELECT llm_text_generate(?, ?) AS result",
    )
    .get(prompt, generateOpts);

  return row?.result ?? "";
}

// ── Public generation API ─────────────────────────────────────────────────

export async function llm(
  tier: ModelTier,
  prompt: string,
  opts?: { maxTokens?: number; model?: string },
): Promise<string> {
  const modelStr = opts?.model ?? modelForTier(tier);

  if (isSqliteAi(modelStr)) {
    const token = modelStr.slice("sqliteai:".length);
    return generateSqliteAi(token, prompt, { maxTokens: opts?.maxTokens });
  }

  const { text } = await generateText({
    model: resolveModel(modelStr),
    prompt,
    maxTokens: opts?.maxTokens,
  });
  return text;
}

export async function llmJson<T = unknown>(
  tier: ModelTier,
  prompt: string,
  opts?: { model?: string; maxTokens?: number },
): Promise<T | null> {
  const text = await llm(tier, prompt, opts);
  try {
    const match = text.match(/[\[{][\s\S]*[\]}]/);
    if (!match) return null;
    return JSON.parse(match[0]) as T;
  } catch {
    return null;
  }
}
