import { cva, type VariantProps } from 'class-variance-authority'
import { X } from 'lucide-react'
import * as ToastPrimitive from '@radix-ui/react-toast'
import * as React from 'react'
import { cn } from '@/lib/cn'

export const ToastProvider = ToastPrimitive.Provider

export const ToastViewport = React.forwardRef<
  React.ElementRef<typeof ToastPrimitive.Viewport>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitive.Viewport>
>(function ToastViewport({ className, ...props }, ref) {
  return (
    <ToastPrimitive.Viewport
      ref={ref}
      className={cn(
        'fixed bottom-0 right-0 z-[100] flex max-h-screen w-[420px] max-w-full flex-col gap-2 p-4 outline-none',
        className,
      )}
      {...props}
    />
  )
})

const toastVariants = cva(
  'group flex w-full items-start gap-3 border bg-panel px-3 py-3 font-mono text-sm text-fg shadow-none data-[state=closed]:animate-toast-out data-[state=open]:animate-toast-in',
  {
    variants: {
      variant: {
        default: 'border-border',
        success: 'border-accent',
        error: 'border-error',
      },
    },
    defaultVariants: { variant: 'default' },
  },
)

export const Toast = React.forwardRef<
  React.ElementRef<typeof ToastPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitive.Root> & VariantProps<typeof toastVariants>
>(function Toast({ className, variant, ...props }, ref) {
  return <ToastPrimitive.Root ref={ref} className={cn(toastVariants({ variant }), className)} {...props} />
})

export const ToastAction = ToastPrimitive.Action

export const ToastClose = React.forwardRef<
  React.ElementRef<typeof ToastPrimitive.Close>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitive.Close>
>(function ToastClose({ className, ...props }, ref) {
  return (
    <ToastPrimitive.Close
      ref={ref}
      className={cn(
        'text-fg-dim transition-colors duration-[60ms] hover:text-fg focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent focus-visible:outline-offset-0',
        className,
      )}
      {...props}
    >
      <X className="h-4 w-4" aria-hidden strokeWidth={1.5} />
    </ToastPrimitive.Close>
  )
})

export const ToastTitle = React.forwardRef<
  React.ElementRef<typeof ToastPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitive.Title>
>(function ToastTitle({ className, ...props }, ref) {
  return (
    <ToastPrimitive.Title ref={ref} className={cn('text-xs font-semibold uppercase tracking-wide text-fg', className)} {...props} />
  )
})

export const ToastDescription = React.forwardRef<
  React.ElementRef<typeof ToastPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof ToastPrimitive.Description>
>(function ToastDescription({ className, ...props }, ref) {
  return <ToastPrimitive.Description ref={ref} className={cn('text-sm text-fg-dim', className)} {...props} />
})

export function Toaster() {
  return (
    <ToastProvider swipeDirection="right">
      <ToastViewport />
    </ToastProvider>
  )
}
