import * as SwitchPrimitives from '@radix-ui/react-switch'
import * as React from 'react'
import { cn } from '@/lib/cn'

export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(function Switch({ className, ...props }, ref) {
  return (
    <SwitchPrimitives.Root
      className={cn(
        'inline-flex h-6 w-10 shrink-0 items-center border border-border bg-canvas px-0.5 transition-colors duration-[60ms]',
        'data-[state=checked]:border-accent data-[state=checked]:bg-accent',
        'focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0 disabled:opacity-40',
        className,
      )}
      {...props}
      ref={ref}
    >
      <SwitchPrimitives.Thumb
        className={cn(
          'pointer-events-none block h-5 w-5 border border-border bg-panel transition-transform duration-[60ms]',
          'data-[state=checked]:translate-x-4 data-[state=checked]:border-accent data-[state=checked]:bg-accent-fg',
        )}
      />
    </SwitchPrimitives.Root>
  )
})
