import type { EventsListQuery, IdeasListQuery, SessionListQuery } from '@/lib/rpc'

export const queryKeys = {
  session: (id: string) => ['session', id] as const,
  sessionMessages: (id: string) => ['session', id, 'messages'] as const,
  sessions: (filter?: SessionListQuery) => ['sessions', filter ?? {}] as const,
  ideas: (filter?: IdeasListQuery) => ['ideas', filter ?? {}] as const,
  blueprints: () => ['blueprints'] as const,
  tools: () => ['tools'] as const,
  health: () => ['health'] as const,
  events: (sessionId?: string, filter?: EventsListQuery) => ['events', sessionId ?? null, filter ?? {}] as const,
  tabs: () => ['tabs'] as const,
  systemConfig: () => ['system', 'config'] as const,
  idea: (filename: string) => ['idea', filename] as const,
  messagesSearch: (q: string) => ['messages', 'search', q] as const,
  codeSearch: (q: string) => ['code', 'search', q] as const,
}
