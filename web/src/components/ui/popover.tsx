import * as P from '@radix-ui/react-popover'
import { cn } from '@/lib/utils'

export const Popover = P.Root
export const PopoverTrigger = P.Trigger
export const PopoverAnchor = P.Anchor
export function PopoverContent({ className, ...p }: P.PopoverContentProps) {
  return (
    <P.Portal>
      <P.Content sideOffset={6} className={cn('z-50 rounded-lg border bg-popover shadow-md focus:outline-none', className)} {...p} />
    </P.Portal>
  )
}
