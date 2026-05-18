import { cva, type VariantProps } from 'class-variance-authority'
import * as React from 'react'
import { cn } from '@/lib/cn'

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 border font-mono transition-colors duration-[60ms] focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0 disabled:pointer-events-none disabled:opacity-40',
  {
    variants: {
      variant: {
        primary:
          'border border-accent bg-accent text-accent-fg uppercase tracking-wide hover:border-border-bold active:bg-panel-active active:text-fg',
        ghost: 'border border-border bg-canvas text-fg hover:bg-panel-active',
        danger: 'border border-error bg-canvas text-error hover:bg-panel-active',
        link: 'border-transparent bg-transparent px-0 py-0 text-accent underline-offset-4 hover:underline',
      },
      size: {
        sm: 'px-2 py-1 text-xs',
        md: 'px-3 py-2 text-sm',
        lg: 'px-4 py-3 text-base',
      },
    },
    defaultVariants: {
      variant: 'primary',
      size: 'md',
    },
  },
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  function Button({ className, variant, size, type = 'button', ...props }, ref) {
    return (
      <button
        ref={ref}
        type={type}
        className={cn(buttonVariants({ variant, size, className }))}
        {...props}
      />
    )
  },
)

export { buttonVariants }
