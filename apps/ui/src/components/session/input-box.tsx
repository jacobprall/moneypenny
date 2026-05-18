import { forwardRef, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/rpc'
import { useQueryClient } from '@tanstack/react-query'
import { queryKeys } from '@/lib/query-keys'

interface SlashCommand {
  name: string
  description: string
  hasArg?: boolean
  execute: (arg: string) => void | Promise<void>
}

export const SessionInputBox = forwardRef<HTMLTextAreaElement, { sessionId: string; disabled?: boolean }>(
  function SessionInputBox({ sessionId, disabled }, ref) {
    const [value, setValue] = useState('')
    const [slashOpen, setSlashOpen] = useState(false)
    const [selectedIdx, setSelectedIdx] = useState(0)
    const popoverRef = useRef<HTMLDivElement>(null)
    const qc = useQueryClient()

    const commands: SlashCommand[] = useMemo(
      () => [
        {
          name: '/clear',
          description: 'clear conversation display',
          execute: () => {
            window.dispatchEvent(new CustomEvent('mp:clear-messages', { detail: { sessionId } }))
          },
        },
        {
          name: '/blueprint',
          description: 'switch blueprint',
          hasArg: true,
          execute: async (arg) => {
            const name = arg.trim()
            if (!name) return
            const detail = await api.sessions.get(sessionId)
            await api.sessions.patchConfig(sessionId, {
              config: { blueprint: name },
              config_version: detail.config_version ?? 0,
            })
            void qc.invalidateQueries({ queryKey: queryKeys.session(sessionId) })
          },
        },
        {
          name: '/pause',
          description: 'pause session',
          execute: () => void api.sessions.pause(sessionId),
        },
        {
          name: '/resume',
          description: 'resume session',
          execute: () => void api.sessions.resume(sessionId),
        },
        {
          name: '/tools',
          description: 'list available tools',
          execute: async () => {
            const res = await api.tools.list()
            const names = res.items.map((t) => t.name).join(', ')
            await api.sessions.inject(sessionId, {
              content: `[system] Available tools: ${names}`,
            })
          },
        },
        {
          name: '/help',
          description: 'show available commands',
          execute: async () => {
            const lines = commands.map((c) => `${c.name} — ${c.description}`)
            await api.sessions.inject(sessionId, {
              content: `[system] Slash commands:\n${lines.join('\n')}`,
            })
          },
        },
      ],
      [sessionId, qc],
    )

    const slashPrefix = value.startsWith('/') ? value.split(/\s/)[0].toLowerCase() : ''
    const filtered = useMemo(() => {
      if (!slashPrefix) return commands
      return commands.filter((c) => c.name.startsWith(slashPrefix))
    }, [slashPrefix, commands])

    useEffect(() => {
      setSelectedIdx(0)
    }, [slashPrefix])

    const executeCommand = useCallback(
      (cmd: SlashCommand) => {
        const parts = value.trim().split(/\s+/)
        const arg = parts.slice(1).join(' ')
        void cmd.execute(arg)
        setValue('')
        setSlashOpen(false)
      },
      [value],
    )

    const submit = useCallback(async () => {
      const v = value.trim()
      if (!v) return

      if (v.startsWith('/')) {
        const cmdName = v.split(/\s/)[0].toLowerCase()
        const match = commands.find((c) => c.name === cmdName)
        if (match) {
          executeCommand(match)
          return
        }
      }

      await api.sessions.inject(sessionId, { content: v })
      setValue('')
    }, [sessionId, value, commands, executeCommand])

    return (
      <div className="relative border-t border-border bg-panel p-3">
        {slashOpen && filtered.length > 0 && (
          <div
            ref={popoverRef}
            className="absolute bottom-full left-3 right-3 mb-1 max-h-56 overflow-y-auto border border-border-bold bg-canvas font-mono text-sm"
          >
            <div className="px-3 py-1.5 text-[10px] uppercase tracking-wider text-fg-faint">
              commands
            </div>
            {filtered.map((cmd, i) => (
              <button
                key={cmd.name}
                type="button"
                className={`flex w-full items-center gap-3 px-3 py-2 text-left transition-colors-fast ${
                  i === selectedIdx ? 'bg-panel-active text-accent' : 'text-fg hover:bg-panel-active'
                }`}
                onMouseEnter={() => setSelectedIdx(i)}
                onMouseDown={(e) => {
                  e.preventDefault()
                  if (cmd.hasArg) {
                    setValue(`${cmd.name} `)
                    setSlashOpen(false)
                  } else {
                    executeCommand(cmd)
                  }
                }}
              >
                <span className="text-accent">{cmd.name}</span>
                <span className="text-fg-dim">{cmd.description}</span>
              </button>
            ))}
          </div>
        )}

        <Textarea
          ref={ref}
          disabled={disabled}
          placeholder="type a message…"
          value={value}
          onChange={(e) => {
            const v = e.target.value
            setValue(v)
            const firstLine = v.split('\n')[0]
            if (firstLine.startsWith('/') && !firstLine.includes(' ')) {
              setSlashOpen(true)
            } else {
              setSlashOpen(false)
            }
          }}
          onKeyDown={(e) => {
            if (slashOpen && filtered.length > 0) {
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setSelectedIdx((i) => (i + 1) % filtered.length)
                return
              }
              if (e.key === 'ArrowUp') {
                e.preventDefault()
                setSelectedIdx((i) => (i - 1 + filtered.length) % filtered.length)
                return
              }
              if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
                e.preventDefault()
                const cmd = filtered[selectedIdx]
                if (cmd) {
                  if (cmd.hasArg) {
                    setValue(`${cmd.name} `)
                    setSlashOpen(false)
                  } else {
                    executeCommand(cmd)
                  }
                }
                return
              }
              if (e.key === 'Escape') {
                e.preventDefault()
                setSlashOpen(false)
                return
              }
            }
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault()
              void submit()
            }
          }}
        />
        <div className="mt-2 flex justify-between gap-2">
          <span className="font-mono text-[10px] text-fg-faint">⏎ send · ⇧⏎ newline · / commands</span>
          <Button type="button" variant="primary" size="sm" disabled={disabled} onClick={() => void submit()}>
            send
          </Button>
        </div>
      </div>
    )
  },
)
