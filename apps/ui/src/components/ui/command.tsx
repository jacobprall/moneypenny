import { Command as CommandPrimitive } from 'cmdk'
import * as React from 'react'
import { cn } from '@/lib/cn'

export const Command = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive>
>(function Command({ className, ...props }, ref) {
  return (
    <CommandPrimitive
      ref={ref}
      className={cn('flex h-full w-full flex-col overflow-hidden border border-border-bold bg-canvas text-fg', className)}
      {...props}
    />
  )
})

export const CommandInput = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.Input>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Input>
>(function CommandInput({ className, ...props }, ref) {
  return (
    <div className="border-b border-border-bold px-4 py-3">
      <CommandPrimitive.Input
        ref={ref}
        className={cn(
          'w-full bg-transparent font-mono text-sm text-fg caret-accent outline-none placeholder:text-fg-faint',
          className,
        )}
        {...props}
      />
    </div>
  )
})

export const CommandList = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.List>
>(function CommandList({ className, ...props }, ref) {
  return (
    <CommandPrimitive.List ref={ref} className={cn('max-h-[60vh] overflow-y-auto p-2 font-mono text-sm', className)} {...props} />
  )
})

export const CommandEmpty = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.Empty>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Empty>
>(function CommandEmpty({ className, ...props }, ref) {
  return <CommandPrimitive.Empty ref={ref} className={cn('px-3 py-6 text-center text-fg-dim', className)} {...props} />
})

export const CommandGroup = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.Group>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Group>
>(function CommandGroup({ className, ...props }, ref) {
  return <CommandPrimitive.Group ref={ref} className={cn('py-2', className)} {...props} />
})

export const CommandItem = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Item>
>(function CommandItem({ className, ...props }, ref) {
  return (
    <CommandPrimitive.Item
      ref={ref}
      className={cn(
        'flex cursor-pointer items-center justify-between gap-3 px-3 py-2 text-fg outline-none',
        'data-[disabled]:pointer-events-none data-[disabled]:opacity-40',
        'data-[selected=true]:bg-panel-active data-[selected=true]:text-accent',
        className,
      )}
      {...props}
    />
  )
})

export function CommandPaletteShell(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  children: React.ReactNode
}) {
  const { open, onOpenChange, children } = props
  if (!open) return null
  return (
    <div
      role="dialog"
      aria-modal
      className="fixed inset-0 z-50 flex flex-col bg-canvas"
      onKeyDown={(e) => e.key === 'Escape' && onOpenChange(false)}
    >
      <div className="flex justify-between border-b border-border px-4 py-2 font-mono text-xs text-fg-dim">
        <span>⌘K</span>
        <span>arrows enter esc</span>
      </div>
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
    </div>
  )
}
