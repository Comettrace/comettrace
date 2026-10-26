import { Settings2 } from 'lucide-react'
import { ALL_NAV } from './nav'
import { NavLink } from '@/lib/router'
import { cn } from '@/lib/utils'

export function MobileTabs() {
  const items = [ALL_NAV[0], ALL_NAV[1], ALL_NAV[2], ALL_NAV[3], { label: 'More', to: '/settings', icon: Settings2 }]
  return (
    <nav aria-label="Primary" className="sticky bottom-0 z-20 flex h-14 shrink-0 border-t bg-sidebar md:hidden">
      {items.map((i) => (
        <NavLink key={i.to} to={i.to} end={i.to === '/'} className={({ isActive }) => cn('flex flex-1 flex-col items-center justify-center gap-0.5 text-[11px] font-semibold text-muted-foreground no-underline', isActive && 'text-foreground')}>
          <i.icon className="size-5" aria-hidden />{i.label}
        </NavLink>
      ))}
    </nav>
  )
}
