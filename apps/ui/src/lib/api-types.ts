/** Client-side shapes aligned with `docs/v2/04-api.md` and related v2 docs. */

export type SessionStatus =
  | 'active'
  | 'paused'
  | 'running'
  | 'completed'
  | 'failed'
  | 'archived'
  | string

export interface Session {
  id: string
  label: string | null
  status: SessionStatus
  cwd: string
  blueprint: string
  idea_id?: string | null
  cost_usd?: number
  config_version?: number
  created_at?: number
  updated_at?: number
  last_activity_at?: number
}

export interface SessionDetail extends Session {
  runs?: RunSummary[]
}

export interface RunSummary {
  id: string
  status: string
  created_at?: number
}

export interface Paginated<T> {
  items: T[]
  nextCursor: string | null
  hasMore: boolean
}

export interface Message {
  id: string
  session_id: string
  run_id?: string | null
  role: 'user' | 'assistant' | 'system' | string
  content: string
  kind?: string
  seq?: number
  created_at?: number
  metadata?: Record<string, unknown>
}

export interface TabRecord {
  id: string
  kind: 'overview' | 'session' | 'ideas' | 'search' | string
  session_id?: string | null
  label: string
  position: number
  active: boolean
  status?: SessionStatus
}

export interface IdeaFrontmatter {
  title?: string
  status?: string
  priority?: string
  tags?: string[]
  spec_session_id?: string | null
  impl_session_ids?: string[]
  created_at?: string
  updated_at?: string
  links?: Array<{ type: string; id: string; note?: string }>
  [key: string]: unknown
}

export interface Idea {
  filename: string
  path?: string
  frontmatter: IdeaFrontmatter
  body: string
}

export interface BlueprintMeta {
  name: string
  title?: string
  description?: string
}

export interface BlueprintDetail extends BlueprintMeta {
  raw?: string
  frontmatter?: Record<string, unknown>
}

export interface ToolMeta {
  name: string
  description?: string
  permissions?: string[]
}

export interface HealthPayload {
  ok: boolean
  db?: string
  today_cost_usd?: number
  active_sessions?: number
  pending_work?: number
  total_knowledge?: number
  pool?: { running?: number; queued?: number }
}

export interface SystemConfig {
  models?: {
    strong?: string
    fast?: string
    local?: string
  }
  ollama_base_url?: string
  sqlite_ai?: {
    model_dir?: string
    context_size?: number
    n_predict?: number
    gpu_layers?: number
  }
  [key: string]: unknown
}

export interface ServerEvent {
  id: number
  type: string
  session_id: string | null
  run_id?: string | null
  blueprint?: string | null
  detail: Record<string, unknown>
  created_at: number
}

export interface CodeSearchHit {
  path: string
  line?: number
  snippet?: string
  score?: number
}

export interface KnowledgeHit {
  id: string
  name?: string
  kind?: string
}

export interface RpcErrorBody {
  error: { code: string; message: string; details?: unknown }
}
