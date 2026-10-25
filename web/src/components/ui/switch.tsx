import * as SwitchPrimitive from '@radix-ui/react-switch'
import { cn } from '@/lib/utils'

export function Switch({ className, label, ...p }: SwitchPrimitive.SwitchProps & { label?: React.ReactNode }) {
  const el = (
    <SwitchPrimitive.Root
      /* items-center is load bearing: an 18px track around a 14px thumb has 4px of slack, and
         without it the thumb stretches to the top edge and sits 2px high. The 2px left and right
         come from the two translations instead. */
      className={cn('relative inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full bg-input transition-colors data-[state=checked]:bg-primary focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-1', className)}
      {...p}
    >
      <SwitchPrimitive.Thumb className="block size-3.5 translate-x-0.5 rounded-full bg-background shadow-sm transition-transform data-[state=checked]:translate-x-[16px]" />
    </SwitchPrimitive.Root>
  )
  if (!label) return el
  return <label className="inline-flex h-8 cursor-pointer items-center gap-2 text-control">{el}{label}</label>
}
