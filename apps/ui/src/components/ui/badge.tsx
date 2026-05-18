import { cva, type VariantProps } from 'class-variance-authority'
import * as React from 'react'
import { cn } from '@/lib/cn'

const badgeVariants = cva(
  'inline-flex items-center border px-2 py-0.5 font-mono text-xs uppercase tracking-wide',
  {
    variants: {
      variant: {
        default: 'border-border text-fg',
        info: 'border-info text-info',
        warn: 'border-warn text-warn',
        error: 'border-error text-error',
        accent: 'border-accent text-accent',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />
}
