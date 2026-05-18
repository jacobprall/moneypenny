/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx,html}'],
  theme: {
    extend: {
      colors: {
        canvas: 'var(--bg)',
        panel: 'var(--bg-elevated)',
        'panel-active': 'var(--bg-active)',
        border: 'var(--border)',
        'border-bold': 'var(--border-bold)',
        fg: 'var(--fg)',
        'fg-dim': 'var(--fg-dim)',
        'fg-faint': 'var(--fg-faint)',
        accent: 'var(--accent)',
        'accent-fg': 'var(--accent-fg)',
        warn: 'var(--warn)',
        error: 'var(--error)',
        info: 'var(--info)',
        success: 'var(--success)',
      },
      borderRadius: {
        DEFAULT: '0',
        sm: '0',
        md: '0',
        lg: '0',
        xl: '0',
        '2xl': '0',
        '3xl': '0',
        full: '0',
      },
      fontFamily: {
        sans: ['Inter', 'ui-sans-serif', 'sans-serif'],
        mono: ['IBM Plex Mono', 'ui-monospace', 'monospace'],
        code: ['JetBrains Mono', 'ui-monospace', 'monospace'],
      },
      keyframes: {
        'pulse-border': {
          '0%, 100%': { opacity: '0.6' },
          '50%': { opacity: '1' },
        },
        'cursor-blink': {
          '0%, 49%': { opacity: '1' },
          '50%, 100%': { opacity: '0' },
        },
        scanline: {
          '0%': { transform: 'translateY(-100%)' },
          '100%': { transform: 'translateY(100%)' },
        },
        'slide-in-right': {
          '0%': { transform: 'translateX(100%)' },
          '100%': { transform: 'translateX(0)' },
        },
        'slide-out-right': {
          '0%': { transform: 'translateX(0)' },
          '100%': { transform: 'translateX(100%)' },
        },
        'toast-in': {
          '0%': { transform: 'translate(100%, 100%)' },
          '100%': { transform: 'translate(0, 0)' },
        },
        'toast-out': {
          '0%': { transform: 'translate(0, 0)' },
          '100%': { transform: 'translate(100%, 100%)' },
        },
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
      },
      animation: {
        'pulse-border': 'pulse-border 1.4s ease-in-out infinite',
        'cursor-blink': 'cursor-blink 1s steps(2, end) infinite',
        scanline: 'scanline 200ms ease-out forwards',
        'slide-in-right': 'slide-in-right 120ms ease-out both',
        'slide-out-right': 'slide-out-right 120ms ease-out both',
        'toast-in': 'toast-in 120ms ease-out both',
        'toast-out': 'toast-out 120ms ease-out both',
        'fade-in': 'fade-in 80ms ease-out both',
      },
    },
  },
  plugins: [],
}
