import { rpcBaseUrl } from '@/lib/rpc'
import type { ServerEvent } from '@/lib/api-types'

type Channel = 'global' | `session:${string}`

type Listener = (event: ServerEvent) => void

function sseOrigin(): string {
  const base = rpcBaseUrl().replace(/\/api$/, '')
  return base.length > 0 ? base : ''
}

function globalEventsUrl(): string {
  return `${sseOrigin()}/api/sse/events`
}

function sessionEventsUrl(id: string): string {
  return `${sseOrigin()}/api/sse/sessions/${id}`
}

function parseDataObject(data: string): ServerEvent | null {
  try {
    const parsed: unknown = JSON.parse(data)
    if (!parsed || typeof parsed !== 'object') return null
    return parsed as ServerEvent
  } catch {
    return null
  }
}

interface SseParsed {
  id: string | null
  event: string
  data: string
}

function parseSseBlocks(text: string): { blocks: SseParsed[]; rest: string } {
  const blocks: SseParsed[] = []
  const chunks = text.split('\n\n')
  const rest = chunks.pop() ?? ''
  for (const chunk of chunks) {
    let id: string | null = null
    let event = 'message'
    const dataLines: string[] = []
    for (const rawLine of chunk.split('\n')) {
      const line = rawLine.replace(/\r$/, '')
      if (line.startsWith('id:')) id = line.slice(3).trim() || null
      else if (line.startsWith('event:')) event = line.slice(6).trim() || 'message'
      else if (line.startsWith('data:')) dataLines.push(line.slice(5).trimStart())
    }
    if (dataLines.length > 0) {
      blocks.push({ id, event, data: dataLines.join('\n') })
    }
  }
  return { blocks, rest }
}

class FetchSseConnection {
  private controller: AbortController | null = null
  private lastId: string | null = null
  private buffer = ''
  private active = false

  constructor(
    private readonly url: string,
    private readonly onEvent: (e: ServerEvent) => void,
  ) {}

  start() {
    this.active = true
    this.stopInner()
    const run = async () => {
      while (this.active) {
        this.controller = new AbortController()
        try {
          const headers: Record<string, string> = { Accept: 'text/event-stream' }
          if (this.lastId) headers['Last-Event-ID'] = this.lastId
          const res = await fetch(this.url, { headers, signal: this.controller.signal })
          if (!res.ok || !res.body) throw new Error(`sse ${res.status}`)
          const reader = res.body.getReader()
          const dec = new TextDecoder()
          for (;;) {
            const { value, done } = await reader.read()
            if (done) break
            this.buffer += dec.decode(value, { stream: true })
            const { blocks, rest } = parseSseBlocks(this.buffer)
            this.buffer = rest
            for (const b of blocks) {
              if (b.id) this.lastId = b.id
              const obj = parseDataObject(b.data)
              if (!obj) continue
              if (!('type' in obj) || typeof (obj as ServerEvent).type !== 'string') {
                ;(obj as ServerEvent).type = b.event
              }
              this.onEvent(obj as ServerEvent)
            }
          }
        } catch {
          if (!this.active) break
          await new Promise((r) => setTimeout(r, 1500))
        }
      }
    }
    void run()
  }

  stop() {
    this.active = false
    this.stopInner()
  }

  private stopInner() {
    this.controller?.abort()
    this.controller = null
  }
}

export class SseManager {
  private static instance: SseManager | undefined

  static get(): SseManager {
    if (!SseManager.instance) SseManager.instance = new SseManager()
    return SseManager.instance
  }

  private readonly listeners = new Map<Channel, Set<Listener>>()
  private globalConn: FetchSseConnection | null = null
  private readonly sessionConns = new Map<string, FetchSseConnection>()
  private closeTimers = new Map<Channel, ReturnType<typeof setTimeout>>()

  private constructor() {}

  subscribe(channel: Channel, listener: Listener): () => void {
    this.cancelCloseTimer(channel)
    let set = this.listeners.get(channel)
    if (!set) {
      set = new Set()
      this.listeners.set(channel, set)
    }
    set.add(listener)
    this.ensureOpen(channel)
    return () => {
      const bucket = this.listeners.get(channel)
      if (!bucket) return
      bucket.delete(listener)
      if (bucket.size === 0) {
        this.listeners.delete(channel)
        this.scheduleClose(channel)
      }
    }
  }

  private cancelCloseTimer(channel: Channel) {
    const t = this.closeTimers.get(channel)
    if (t) clearTimeout(t)
    this.closeTimers.delete(channel)
  }

  private scheduleClose(channel: Channel) {
    this.cancelCloseTimer(channel)
    const timer = setTimeout(() => {
      this.closeTimers.delete(channel)
      if (channel === 'global') {
        this.globalConn?.stop()
        this.globalConn = null
        return
      }
      if (channel.startsWith('session:')) {
        const id = channel.slice('session:'.length)
        this.sessionConns.get(id)?.stop()
        this.sessionConns.delete(id)
      }
    }, 5000)
    this.closeTimers.set(channel, timer)
  }

  private emit(channel: Channel, event: ServerEvent) {
    const bucket = this.listeners.get(channel)
    if (!bucket) return
    for (const fn of bucket) fn(event)
  }

  private ensureOpen(channel: Channel) {
    if (channel === 'global') {
      if (this.globalConn) return
      const conn = new FetchSseConnection(globalEventsUrl(), (e) => this.emit('global', e))
      this.globalConn = conn
      conn.start()
      return
    }
    if (channel.startsWith('session:')) {
      const id = channel.slice('session:'.length)
      if (this.sessionConns.has(id)) return
      const conn = new FetchSseConnection(sessionEventsUrl(id), (e) => this.emit(channel, e))
      this.sessionConns.set(id, conn)
      conn.start()
    }
  }
}
