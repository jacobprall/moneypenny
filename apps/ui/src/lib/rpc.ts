import { hc } from 'hono/client'
import type { AppType } from '@moneypenny/api'
import type {
  BlueprintDetail,
  BlueprintMeta,
  HealthPayload,
  Idea,
  Message,
  RpcErrorBody,
  Session,
  SessionDetail,
  SystemConfig,
  TabRecord,
  ToolMeta,
} from '@/lib/api-types'

const envBase = import.meta.env.VITE_API_BASE ?? ''

export function rpcBaseUrl(): string {
  const trimmed = envBase.replace(/\/$/, '')
  if (trimmed.length > 0) return `${trimmed}/api`
  return '/api'
}

export const rpc = hc<AppType>(rpcBaseUrl())

async function parseJson<T>(res: Response): Promise<T> {
  const data: unknown = await res.json()
  if (!res.ok) {
    const err = data as RpcErrorBody
    const msg = err.error?.message ?? res.statusText
    throw new Error(msg)
  }
  return data as T
}

type PostWithJson<P extends Record<string, string>, J> = (arg: { param: P; json: J }) => Promise<Response>
type PatchWithJson<P extends Record<string, string>, J> = (arg: { param: P; json: J }) => Promise<Response>
type PatchBodyOnly<J> = (arg: { json: J }) => Promise<Response>
type GetWithQuery<P extends Record<string, string>, Q extends Record<string, string | undefined>> = (arg: {
  param: P
  query?: Q
}) => Promise<Response>

export interface SessionListQuery {
  status?: string
  blueprint?: string
  label?: string
  cursor?: string
  limit?: number
}

export interface MessageListQuery {
  cursor?: string
  limit?: number
  direction?: 'before' | 'after'
}

export interface IdeasListQuery {
  status?: string
  tags?: string
  cwd?: string
}

export interface EventsListQuery {
  cursor?: string
  limit?: number
  type?: string
  session_id?: string
}

export const api = {
  sessions: {
    list: async (query?: SessionListQuery) => {
      const res = await rpc.sessions.$get({ query: query as Record<string, string | undefined> })
      return parseJson<{ items: Session[]; nextCursor: string | null; hasMore: boolean }>(res)
    },
    get: async (id: string) => {
      const res = await rpc.sessions[':id'].$get({ param: { id } })
      return parseJson<SessionDetail>(res)
    },
    create: async (input: {
      cwd: string
      blueprint?: string
      label?: string
      task?: string
      ideaId?: string
    }) => {
      const res = await rpc.sessions.$post({ json: input })
      return parseJson<Session>(res)
    },
    inject: async (id: string, body: { content: string; role?: string }) => {
      const res = await (
        rpc.sessions[':id']['inject'].$post as PostWithJson<{ id: string }, { content: string; role?: string }>
      )({ param: { id }, json: body })
      return parseJson<{ ok: boolean }>(res)
    },
    patchConfig: async (id: string, body: { config: Record<string, unknown>; config_version: number }) => {
      const res = await (
        rpc.sessions[':id']['config'].$patch as PatchWithJson<
          { id: string },
          { config: Record<string, unknown>; config_version: number }
        >
      )({ param: { id }, json: body })
      return parseJson<Session>(res)
    },
    pause: async (id: string) => {
      const res = await rpc.sessions[':id']['pause'].$post({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
    resume: async (id: string) => {
      const res = await rpc.sessions[':id']['resume'].$post({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
    complete: async (id: string) => {
      const res = await rpc.sessions[':id']['complete'].$post({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
    archive: async (id: string) => {
      const res = await rpc.sessions[':id']['archive'].$post({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
    remove: async (id: string) => {
      const res = await rpc.sessions[':id'].$delete({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
  },
  messages: {
    search: async (q: string) => {
      const res = await rpc.messages.$get({ query: { q } })
      return parseJson<{ items: Message[]; nextCursor: string | null; hasMore: boolean }>(res)
    },
    listBySession: async (id: string, query?: MessageListQuery) => {
      const res = await (
        rpc.messages['by-session'][':id'].$get as GetWithQuery<
          { id: string },
          Record<string, string | undefined>
        >
      )({
        param: { id },
        query: query as Record<string, string | undefined>,
      })
      return parseJson<{ items: Message[]; nextCursor: string | null; hasMore: boolean }>(res)
    },
  },
  blueprints: {
    list: async () => {
      const res = await rpc.blueprints.$get()
      return parseJson<{ items: BlueprintMeta[] }>(res)
    },
    get: async (name: string) => {
      const res = await rpc.blueprints[':name'].$get({ param: { name } })
      return parseJson<BlueprintDetail>(res)
    },
    reload: async () => {
      const res = await rpc.blueprints.reload.$post()
      return parseJson<{ ok: boolean }>(res)
    },
  },
  ideas: {
    list: async (query?: IdeasListQuery) => {
      const res = await rpc.ideas.$get({ query: query as Record<string, string | undefined> })
      return parseJson<{ items: Idea[] }>(res)
    },
    get: async (filename: string) => {
      const res = await rpc.ideas[':filename'].$get({ param: { filename } })
      return parseJson<Idea>(res)
    },
    create: async (input: { filename: string; body: string }) => {
      const res = await rpc.ideas.$post({ json: input })
      return parseJson<Idea>(res)
    },
    update: async (filename: string, input: { body?: string; frontmatter?: Record<string, unknown> }) => {
      const res = await (
        rpc.ideas[':filename'].$patch as PatchWithJson<
          { filename: string },
          { body?: string; frontmatter?: Record<string, unknown> }
        >
      )({ param: { filename }, json: input })
      return parseJson<Idea>(res)
    },
    remove: async (filename: string) => {
      const res = await rpc.ideas[':filename'].$delete({ param: { filename } })
      return parseJson<{ ok: boolean }>(res)
    },
  },
  agents: {
    launch: async (input: {
      cwd: string
      blueprint?: string
      label?: string
      task?: string
      idea_id?: string
    }) => {
      const res = await rpc.agents.launch.$post({ json: input })
      return parseJson<{ session: Session }>(res)
    },
    status: async () => {
      const res = await rpc.agents.status.$get()
      return parseJson<Record<string, unknown>>(res)
    },
    kill: async (sessionId: string) => {
      const res = await rpc.agents.kill[':sessionId'].$post({ param: { sessionId } })
      return parseJson<{ ok: boolean }>(res)
    },
  },
  tools: {
    list: async () => {
      const res = await rpc.tools.$get()
      return parseJson<{ items: ToolMeta[] }>(res)
    },
  },
  code: {
    search: async (q: string) => {
      const res = await rpc.code.search.$get({ query: { q } })
      return parseJson<{ items: unknown[] }>(res)
    },
    index: async () => {
      const res = await rpc.code.index.$post()
      return parseJson<{ ok: boolean }>(res)
    },
  },
  files: {
    list: async (path: string) => {
      const res = await rpc.files.list.$get({ query: { path } })
      return parseJson<{ items: { name: string; path: string; kind: string }[] }>(res)
    },
  },
  knowledge: {
    skills: async () => {
      const res = await rpc.knowledge.skills.$get()
      return parseJson<{ items: unknown[] }>(res)
    },
    conventions: async () => {
      const res = await rpc.knowledge.conventions.$get()
      return parseJson<{ items: unknown[] }>(res)
    },
    pointers: async (sessionId?: string) => {
      const res = await rpc.knowledge.pointers.$get({
        query: sessionId ? { session_id: sessionId } : undefined,
      })
      return parseJson<{ items: unknown[] }>(res)
    },
  },
  events: {
    list: async (query?: EventsListQuery) => {
      const res = await rpc.events.$get({ query: query as Record<string, string | undefined> })
      return parseJson<{ items: unknown[]; nextCursor: string | null; hasMore: boolean }>(res)
    },
  },
  tabs: {
    list: async () => {
      const res = await rpc.tabs.$get()
      return parseJson<{ items: TabRecord[] }>(res)
    },
    open: async (input: {
      kind: TabRecord['kind']
      session_id?: string | null
      label: string
    }) => {
      const res = await rpc.tabs.$post({ json: input })
      return parseJson<TabRecord>(res)
    },
    patch: async (
      id: string,
      input: { position?: number; active?: boolean; label?: string; status?: string },
    ) => {
      const res = await (
        rpc.tabs[':id'].$patch as PatchWithJson<
          { id: string },
          { position?: number; active?: boolean; label?: string; status?: string }
        >
      )({ param: { id }, json: input })
      return parseJson<TabRecord>(res)
    },
    close: async (id: string) => {
      const res = await rpc.tabs[':id'].$delete({ param: { id } })
      return parseJson<{ ok: boolean }>(res)
    },
  },
  system: {
    health: async () => {
      const res = await rpc.system.health.$get()
      return parseJson<HealthPayload>(res)
    },
    getConfig: async () => {
      const res = await rpc.system.config.$get()
      return parseJson<SystemConfig>(res)
    },
    setConfig: async (patch: Record<string, unknown>) => {
      const kv = Object.fromEntries(
        Object.entries(patch).map(([k, v]) => [k, typeof v === 'string' ? v : JSON.stringify(v)]),
      )
      const res = await (rpc.system.config.$patch as unknown as PatchBodyOnly<{ kv: Record<string, string> }>)({
        json: { kv },
      })
      return parseJson<SystemConfig>(res)
    },
  },
}
