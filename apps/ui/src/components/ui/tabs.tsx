import * as TabsPrimitive from '@radix-ui/react-tabs'
import * as React from 'react'
import { cn } from '@/lib/cn'

export const Tabs = TabsPrimitive.Root

export const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(function TabsList({ className, ...props }, ref) {
  return (
    <TabsPrimitive.List
      ref={ref}
      className={cn('flex gap-1 border-b border-border bg-canvas px-2', className)}
      {...props}
    />
  )
})

export type TabsTriggerProps = React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger> & {
  /** Pulse border when the tab is in a running state (Radix owns `data-state` on this node). */
  running?: boolean
}

export const TabsTrigger = React.forwardRef<React.ElementRef<typeof TabsPrimitive.Trigger>, TabsTriggerProps>(
  function TabsTrigger({ className, running, ...props }, ref) {
    return (
      <TabsPrimitive.Trigger
        ref={ref}
        data-running={running ? '' : undefined}
        className={cn(
          'border-b-2 border-transparent px-3 py-2 font-mono text-xs font-medium uppercase tracking-wide text-fg-dim transition-colors duration-[60ms]',
          'hover:text-fg focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0',
          'data-[state=active]:border-accent data-[state=active]:text-fg data-[state=active]:font-bold',
          '[&[data-running]]:animate-pulse-border',
          className,
        )}
        {...props}
      />
    )
  },
)

export const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(function TabsContent({ className, ...props }, ref) {
  return (
    <TabsPrimitive.Content
      ref={ref}
      className={cn('p-4 font-mono text-sm text-fg outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0', className)}
      {...props}
    />
  )
})
