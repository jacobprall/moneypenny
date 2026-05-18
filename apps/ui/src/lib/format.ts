export function formatUsd(amount: number | undefined): string {
  if (amount === undefined || Number.isNaN(amount)) return '$0.0000'
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 4,
    maximumFractionDigits: 4,
  }).format(amount)
}

export function formatDurationMs(ms: number | undefined): string {
  if (ms === undefined || Number.isNaN(ms)) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  const s = ms / 1000
  if (s < 60) return `${s.toFixed(1)}s`
  const m = Math.floor(s / 60)
  const r = s - m * 60
  return `${m}m ${r.toFixed(0)}s`
}

export function formatRelativeTime(ts: number | undefined): string {
  if (ts === undefined || Number.isNaN(ts)) return '—'
  const now = Date.now()
  const ms = ts < 1e12 ? ts * 1000 : ts
  const sec = Math.round((now - ms) / 1000)
  const abs = Math.abs(sec)
  const label =
    abs < 60
      ? `${abs}s`
      : abs < 3600
        ? `${Math.floor(abs / 60)}m`
        : abs < 86400
          ? `${Math.floor(abs / 3600)}h`
          : `${Math.floor(abs / 86400)}d`
  return sec >= 0 ? `${label} ago` : `in ${label}`
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n)) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i += 1
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export function truncateUtf8(s: string, maxBytes: number): { text: string; truncated: boolean } {
  const enc = new TextEncoder()
  if (enc.encode(s).length <= maxBytes) return { text: s, truncated: false }
  let lo = 0
  let hi = s.length
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2)
    const slice = s.slice(0, mid)
    if (enc.encode(slice).length <= maxBytes) lo = mid
    else hi = mid - 1
  }
  return { text: s.slice(0, lo), truncated: true }
}
