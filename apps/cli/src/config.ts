import { existsSync, mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join as pathJoin } from "node:path";
import { inferProvider, type ProviderName } from "@moneypenny/loop";

export interface ResolvedConfig {
  provider: ProviderName;
  apiKey: string;
  model: string;
  maxCostPerSession?: number;
  confirmDestructive: boolean;
  autoIndex: boolean;
}

let _globalcache: Record<string, unknown> | null | undefined;

export function globalConfigPath(): string {
  return pathJoin(homedir(), ".mp", "config.json");
}

function loadGlobalRaw(): Record<string, unknown> {
  if (_globalcache !== undefined) {
    return _globalcache ?? {};
  }
  const p = globalConfigPath();
  if (!existsSync(p)) {
    _globalcache = null;
    return {};
  }
  try {
    const raw = JSON.parse(readFileSync(p, "utf8")) as unknown;
    if (typeof raw === "object" && raw !== null && !Array.isArray(raw)) {
      _globalcache = raw as Record<string, unknown>;
      return _globalcache;
    }
  } catch {
    /* skip */
  }
  _globalcache = null;
  return {};
}

export function invalidateGlobalConfigCache(): void {
  _globalcache = undefined;
}

export function writeGlobalConfigKey(key: string, value: unknown): void {
  const p = globalConfigPath();
  let obj: Record<string, unknown> = {};
  if (existsSync(p)) {
    try {
      const raw = JSON.parse(readFileSync(p, "utf8")) as unknown;
      if (typeof raw === "object" && raw !== null && !Array.isArray(raw)) {
        obj = raw as Record<string, unknown>;
      }
    } catch { /* overwrite corrupt file */ }
  }
  obj[key] = value;
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, `${JSON.stringify(obj, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
  try { chmodSync(p, 0o600); } catch { /* best effort */ }
  invalidateGlobalConfigCache();
}

/** Read a global config value as a string. Public for theme/config access. */
export function readGlobalConfig(key: string): string | undefined {
  const obj = loadGlobalRaw();
  if (!(key in obj)) return undefined;
  const v = obj[key];
  if (v === undefined || v === null) return undefined;
  if (typeof v === "boolean" || typeof v === "number") return String(v);
  if (typeof v === "string") return v;
  try {
    return JSON.stringify(v);
  } catch {
    return undefined;
  }
}

const PROVIDER_KEY_MAP: Record<ProviderName, { envVar: string; configKey: string; label: string }> = {
  anthropic: { envVar: "ANTHROPIC_API_KEY", configKey: "anthropic_api_key", label: "Anthropic" },
  openai: { envVar: "OPENAI_API_KEY", configKey: "openai_api_key", label: "OpenAI" },
  google: { envVar: "GOOGLE_API_KEY", configKey: "google_api_key", label: "Google" },
};

function resolveApiKey(provider: ProviderName): string | undefined {
  const spec = PROVIDER_KEY_MAP[provider];
  return process.env[spec.envVar] ?? readGlobalConfig(spec.configKey);
}

/** Returns providers that have a configured API key. */
export function availableProviders(): ProviderName[] {
  return (["anthropic", "openai", "google"] as ProviderName[]).filter(
    (p) => resolveApiKey(p) != null,
  );
}

export function resolveConfig(flags: Partial<ResolvedConfig>): ResolvedConfig {
  const model =
    flags.model ??
    process.env.MP_MODEL ??
    readGlobalConfig("model") ??
    "claude-sonnet-4-6";

  const VALID_PROVIDERS: ProviderName[] = ["anthropic", "openai", "google"];
  const rawProvider = flags.provider ?? readGlobalConfig("provider");
  const provider: ProviderName =
    rawProvider && VALID_PROVIDERS.includes(rawProvider as ProviderName)
      ? (rawProvider as ProviderName)
      : inferProvider(model);

  const apiKey = flags.apiKey ?? resolveApiKey(provider);
  if (!apiKey) {
    const spec = PROVIDER_KEY_MAP[provider];
    throw new Error(
      `No ${spec.label} API key found. Set ${spec.envVar} or run \`mp config set ${spec.configKey} <key>\``,
    );
  }

  const maxParsed = parseFloat(readGlobalConfig("max_cost_per_session") ?? "0");

  const confirmDestructive =
    flags.confirmDestructive ??
    (readGlobalConfig("confirm_destructive") !== "false" && readGlobalConfig("confirm_destructive") !== "0");

  const autoIndex =
    flags.autoIndex ?? (readGlobalConfig("auto_index") !== "false" && readGlobalConfig("auto_index") !== "0");

  return {
    provider,
    apiKey,
    model,
    maxCostPerSession: flags.maxCostPerSession ?? (Number.isFinite(maxParsed) && maxParsed > 0 ? maxParsed : undefined),
    confirmDestructive,
    autoIndex,
  };
}
