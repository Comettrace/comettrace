import { cn } from '@/lib/utils'

export function Segmented<T extends string>({ value, onChange, options, label, size = 'default' }: { value: T; onChange: (v: T) => void; options: { value: T; label: React.ReactNode }[]; label: string; size?: 'default' | 'sm' }) {
  return (
    <div role="group" aria-label={label} className="inline-flex gap-0.5 rounded-md border bg-muted p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
          className={cn(
            'inline-flex cursor-pointer items-center gap-1.5 rounded-sm px-2.5 font-semibold text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring',
            size === 'sm' ? 'h-[22px] text-[11.5px] px-2' : 'h-[26px] text-[12.5px]',
            o.value === value && 'bg-background text-foreground shadow-sm',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}
