import { cva, type VariantProps } from 'class-variance-authority'
import * as React from 'react'
import { cn } from '@/lib/cn'

const cardVariants = cva('border bg-panel p-4 font-mono text-sm text-fg transition-colors duration-[60ms]', {
  variants: {
    variant: {
      default: 'border-border',
      active: 'border-border-bold bg-panel-active',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
})

export interface CardProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof cardVariants> {}

export function Card({ className, variant, ...props }: CardProps) {
  return <div className={cn(cardVariants({ variant }), className)} {...props} />
}
