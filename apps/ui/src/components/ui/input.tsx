import * as React from 'react'
import { cn } from '@/lib/cn'

export type InputProps = React.InputHTMLAttributes<HTMLInputElement>

export const Input = React.forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, type = 'text', ...props },
  ref,
) {
  return (
    <input
      type={type}
      ref={ref}
      className={cn(
        'w-full border border-border bg-canvas px-3 py-2 font-mono text-sm text-fg caret-accent',
        'transition-colors duration-[60ms] focus-visible:border-accent focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0',
        'placeholder:text-fg-faint disabled:cursor-not-allowed disabled:opacity-40',
        className,
      )}
      {...props}
    />
  )
})
