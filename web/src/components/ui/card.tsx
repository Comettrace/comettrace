import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export const Card = ({ className, ...p }: ComponentProps<'div'>) => <div className={cn('rounded-lg border bg-card text-card-foreground', className)} {...p} />
export const CardHeader = ({ className, ...p }: ComponentProps<'div'>) => <div className={cn('flex items-center gap-3 border-b px-4 py-3', className)} {...p} />
export const CardTitle = ({ className, ...p }: ComponentProps<'h2'>) => <h2 className={cn('text-[15px] font-bold leading-6', className)} {...p} />
