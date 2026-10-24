import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

const base =
  'flex h-8 w-full items-center gap-2 rounded-md border border-input bg-background px-2.5 text-control text-foreground shadow-sm placeholder:text-faint-foreground focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1 aria-invalid:border-destructive disabled:opacity-55 disabled:bg-muted'

export function Input({ className, mono, ...p }: ComponentProps<'input'> & { mono?: boolean }) {
  return <input className={cn(base, mono && 'font-mono', className)} {...p} />
}

export function Textarea({ className, ...p }: ComponentProps<'textarea'>) {
  return <textarea className={cn(base, 'h-auto min-h-24 py-2 leading-[19px] resize-y', className)} {...p} />
}

export function InputGroup({ className, children }: { className?: string; children: React.ReactNode }) {
  return (
    <div className={cn(base, 'focus-within:outline-2 focus-within:outline-ring focus-within:outline-offset-1 [&_input]:outline-none [&_input]:bg-transparent [&_input]:w-full [&_input]:min-w-0 [&_svg]:size-3.5 [&_svg]:text-muted-foreground', className)}>
      {children}
    </div>
  )
}

export function NativeSelect({ className, ...p }: ComponentProps<'select'>) {
  return <select className={cn(base, 'cursor-pointer pr-7', className)} {...p} />
}

export function Field({ label, hint, htmlFor, children, className }: { label: React.ReactNode; hint?: React.ReactNode; htmlFor?: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn('flex min-w-0 flex-col gap-1.5', className)}>
      <label htmlFor={htmlFor} className="text-control font-semibold text-foreground">{label}</label>
      {children}
      {hint ? <span className="text-caption text-muted-foreground">{hint}</span> : null}
    </div>
  )
}
