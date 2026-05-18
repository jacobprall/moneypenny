import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import * as React from 'react'
import { cn } from '@/lib/cn'

export function TooltipProvider({
  delayDuration = 1,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return <TooltipPrimitive.Provider delayDuration={delayDuration} {...props} />
}

export const Tooltip = TooltipPrimitive.Root
export const TooltipTrigger = TooltipPrimitive.Trigger

export const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(function TooltipContent({ className, sideOffset = 6, ...props }, ref) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        ref={ref}
        sideOffset={sideOffset}
        className={cn(
          'z-50 max-w-xs border border-y-border border-l-accent border-r-border bg-panel px-2 py-1 font-mono text-xs text-fg shadow-none animate-fade-in',
          'focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0',
          className,
        )}
        {...props}
      />
    </TooltipPrimitive.Portal>
  )
})
