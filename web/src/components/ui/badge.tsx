import { cva, type VariantProps } from 'class-variance-authority'
import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-sm border font-semibold whitespace-nowrap [&_svg]:size-3',
  {
    variants: {
      variant: {
        neutral: 'bg-muted text-muted-foreground border-border',
        outline: 'bg-transparent text-muted-foreground border-border',
        accent: 'bg-accent-subtle text-accent-text border-transparent',
        success: 'bg-success-subtle text-success-text border-success-border',
        warning: 'bg-warning-subtle text-warning-text border-warning-border',
        destructive: 'bg-destructive-subtle text-destructive-text border-destructive-border',
      },
      size: { default: 'h-5 px-1.5 text-[11.5px]', xs: 'h-4 px-1 text-[10px]' },
      mono: { true: 'font-mono font-medium text-[11px]', false: '' },
    },
    defaultVariants: { variant: 'neutral', size: 'default', mono: false },
  },
)

export function Badge({ className, variant, size, mono, ...props }: ComponentProps<'span'> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ variant, size, mono }), className)} {...props} />
}
