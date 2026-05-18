# Design Language

Sleek, modern, shadcn-grounded — but with sharp edges and flat surfaces. Retro arcade vibe, kept tasteful: the feel of a high-contrast terminal or a CRT phosphor display, restrained, never cheeky.

## Principles

1. **Sharp over soft.** No rounded corners, no shadows, no glassmorphism. Right angles and 1px borders.
2. **Flat over deep.** Surfaces sit on the same plane. Hierarchy is established by border weight, color contrast, and typography — not depth.
3. **Mono first.** Monospace is the primary type for everything except long-form prose.
4. **One accent.** A single high-contrast accent color carries all interactive emphasis. No second-tier accents.
5. **Density with breathing room.** Information-dense like a terminal, but with measured whitespace so nothing feels cramped.
6. **Restrained motion.** No bounces, no transitions over 150ms, no animated illustrations. A blinking cursor on input. A border-pulse on running state. That's it.
7. **Always dark.** No light mode. The aesthetic is a CRT in a dark room.

## Type System

| Role | Family | Weight | Notes |
|------|--------|--------|-------|
| Brand / wordmark | `IBM Plex Mono` | 700 | All-caps, letter-spacing 0.05em |
| Headings | `IBM Plex Mono` | 600 | All-caps, letter-spacing 0.05em |
| UI / labels | `IBM Plex Mono` | 500 | Mixed case |
| Body / prose | `Inter` | 400 | Long-form text only (markdown bodies, descriptions) |
| Code | `JetBrains Mono` | 400 | In code blocks, diffs, terminal output |
| Numerics / costs | `IBM Plex Mono` (tabular) | 500 | `font-variant-numeric: tabular-nums` |

Sizes (Tailwind scale):
- `text-xs` (12px) — meta, timestamps, badges
- `text-sm` (14px) — default UI
- `text-base` (16px) — message content
- `text-lg` (18px) — section headings
- `text-2xl` (24px) — page titles
- `text-4xl` (36px) — top-bar wordmark, big stats

Line height: 1.5 for prose, 1.4 for UI, 1.2 for headings.

## Color Tokens

CSS variables, dark-only. All colors expressed in OKLCH for perceptual stability.

```css
:root {
  /* Surfaces */
  --bg:           oklch(0.12 0.005 230);  /* near-black, slight cool tint */
  --bg-elevated:  oklch(0.16 0.005 230);  /* subtle lift, no shadow */
  --bg-active:    oklch(0.20 0.005 230);  /* hover/selected */

  /* Borders */
  --border:       oklch(0.28 0.005 230);  /* default 1px */
  --border-bold:  oklch(0.42 0.005 230);  /* emphasis */

  /* Text */
  --fg:           oklch(0.92 0.01  90);   /* warm off-white (phosphor) */
  --fg-dim:       oklch(0.65 0.01  90);
  --fg-faint:     oklch(0.45 0.005 90);

  /* Accent — single high-contrast accent */
  --accent:       oklch(0.78 0.18  140);  /* CRT green-cyan */
  --accent-fg:    oklch(0.15 0.005 230);  /* text on accent */

  /* Semantic */
  --warn:         oklch(0.80 0.15  85);   /* amber */
  --error:        oklch(0.65 0.20  25);   /* red */
  --info:         oklch(0.78 0.12  220);  /* cyan */
  --success:      var(--accent);
}
```

Use of color is constrained:
- `--accent` for primary buttons, focus rings, "running" indicators, links.
- `--warn` for HITL paused state, budget warnings.
- `--error` for failed runs, destructive actions, errors.
- `--info` for tags, secondary badges.
- Everything else is greyscale.

## Sharpness

Sharp by default everywhere:

```css
* { border-radius: 0; }
```

shadcn primitives are forked or overridden to remove `rounded-*`. Where shadcn uses ring + offset for focus, we use `outline: 1px solid var(--accent)` with `outline-offset: 0`.

Borders are 1px solid by default. Selected/active state uses `--border-bold` or accent. Never use shadow for selection — use border.

## Surfaces & Elevation

There is no elevation in pixels. There is elevation in **border weight** and **background**:

| Layer | Background | Border |
|-------|------------|--------|
| Page | `--bg` | none |
| Panel / card | `--bg-elevated` | `1px solid --border` |
| Active / focused | `--bg-active` | `1px solid --border-bold` or accent |
| Modal / drawer | `--bg-elevated` | `1px solid --border-bold` |

Modals don't dim the page; they slide in from the right (drawer) with a thin accent border on the leading edge.

## Iconography

Lucide icons throughout. Stroke width 1.5 (default). Never filled. Sized to match adjacent type (16px for `text-sm`, 14px for `text-xs`).

For status indicators in the tab bar, prefer text glyphs over icons (`●`, `▶`, `⏸`, `✓`, `!`) — they read as monospace and align with surrounding text.

## Motion

Limit set:

| Effect | Use | Duration |
|--------|-----|----------|
| Cursor blink | Input boxes | 1s, infinite |
| Border pulse | Running state on tabs and tool calls | 1.4s, infinite, opacity-only |
| Fade-in | New messages, new tabs | 80ms |
| Slide-in | Drawer/modal | 120ms ease-out |
| Color transition | Hover states | 60ms |

No layout animation. No spring physics. No "bouncy" anything. The pulse is opacity-only (0.6 → 1.0), never scale or position — to read as electronic, not organic.

A subtle one-time effect on app boot: a 200ms fade from black with a single horizontal scanline sweep (top to bottom). One-shot, never repeated. This is the only "arcade" flourish — it sets the tone, then gets out of the way.

## Components

shadcn primitives, modified for sharpness, dark, and mono:

| Primitive | Modifications |
|-----------|---------------|
| Button | square corners; `--accent` bg for primary; `--border` outline for ghost; mono caps for primary CTA labels |
| Input | square; bg `--bg`; border `--border`; focus border `--accent`; cursor blinks |
| Dialog | drawer-style; slide from right; accent border on leading edge |
| Drawer | as Dialog default |
| Card | square, flat, single border |
| Tabs | underline-only style; active tab has accent underline + bold text |
| Table | TanStack Table with shadcn cells; row hover `--bg-active`; sortable headers in mono caps |
| Badge | square, single border, no background by default |
| Toast | square, slide from bottom-right, accent border for success, error border for fail |
| Command (⌘K) | full-screen overlay with mono input; results in monospace lists |
| Tooltip | mono, no border-radius, accent border-left (1px), 1ms delay |
| Switch | square, no rounded thumb, accent fill for on |
| Select | square menu, mono items, accent for selected |

## Density & Spacing

Tailwind spacing tokens, used consistently:

| Use | Token |
|-----|-------|
| Component internal padding | `px-3 py-2` (12px / 8px) |
| Card padding | `p-4` (16px) |
| Section gap | `gap-4` or `gap-6` (16px / 24px) |
| Page margin | `px-6 py-4` |

Everything fits more than it would in a typical SaaS app, but never at the cost of readability. Measure body text at ~70 chars/line max in long-form views.

## Hover & Focus

- Hover: background shift to `--bg-active`, no scale, no shadow, no underline (unless link).
- Focus: 1px solid accent outline, no offset.
- Active (mousedown): same as focus, plus background shift.
- Disabled: `opacity: 0.4`, no events.

Focus is always visible. Mouse and keyboard treated equally.

## Empty States

Mono-only, single line + optional secondary line:

```
NO ACTIVE SESSIONS
press cmd-n to start one
```

No illustrations. No "look, an empty box" metaphors.

## Loading States

Three forms:

1. **Determinate (progress)**: a 1px accent bar animating left-to-right; never use rounded progress bars.
2. **Indeterminate (working)**: opacity pulse on the affected element's border.
3. **Skeleton (initial paint)**: 1px-bordered placeholder boxes with `--bg-elevated` fill, no shimmer.

Spinners are forbidden.

## Error States

- Inline errors: text in `--error`, no icon, prefixed with `! ` or wrapped in a 1px error-border block.
- Full-page errors: monospace, single-column, no illustration. Title in caps, description in mono, action buttons square.

## Accessibility

- Focus ring always visible (1px solid accent, no offset).
- All interactive elements reachable via Tab.
- Hover-only affordances are mirrored to focus.
- Mono type is high-legibility at 14px+.
- Contrast ratio ≥ 7:1 for body text against bg (verified via `--fg` and `--bg`).
- Status conveyed by text/glyph + color, never color alone.

## Examples (intent, not pixels)

A session tab when running:

```
[▶ AUTH IMPL]
```

The bracketed segment uses `--bg-elevated`, no border on the tab itself; the active tab has a 2px accent underline. The `▶` glyph pulses opacity 0.6→1.0 at 1.4s.

A tool call card collapsed:

```
▸ search_code(pattern="middleware")              · 234ms · 3 hits
```

Single-line, square box, 1px border. Click expands to show full args + result with `▾`.

A pause notice:

```
┌──────────────────────────────────────────────────────┐
│ ⏸ paused · awaiting human input · "use jwt or sessions?"
│ [JWT]  [Sessions]                                     │
└──────────────────────────────────────────────────────┘
```

Full-width, `--warn` border, mono text, square buttons.

## Implementation Notes

- shadcn primitives copied into `apps/ui/src/components/ui/` and modified directly. We don't ship the unmodified shadcn for sharpness.
- A single `globals.css` defines tokens, base resets (`* { border-radius: 0 }`), and font imports.
- Tailwind config maps tokens (`bg-canvas`, `border-default`, `text-fg`, `text-fg-dim`, `accent`, etc.) so utility classes read naturally.
- Self-hosted fonts (no Google Fonts) under `apps/ui/public/fonts/`.
