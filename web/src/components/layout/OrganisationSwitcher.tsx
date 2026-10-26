import { Building2, ChevronDown } from 'lucide-react'
import { useState } from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useOrganisations } from '@/auth/session'

const orgLabel = (name: string | undefined) => name ?? 'no organisation'

export function OrganisationSwitcher() {
  const { list, active } = useOrganisations()
  const [chosen, setChosen] = useState<string | null>(null)
  const current = list.find((m) => m.org === chosen) ?? active
  if (list.length < 2) return <span className="hidden whitespace-nowrap font-semibold sm:inline">{orgLabel(active?.name)}</span>
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" aria-haspopup="listbox"><Building2 /><span>{orgLabel(current?.name)}</span><ChevronDown className="text-muted-foreground" /></Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[300px] p-1">
        {list.map((m) => (
          <button key={m.org} type="button" onClick={() => setChosen(m.org)} className="flex h-[30px] w-full cursor-pointer items-center gap-2 rounded-sm px-2 text-left text-control hover:bg-muted">
            <span className="flex-1 truncate">{orgLabel(m.name)}</span>
            <Badge variant="outline">{m.role}</Badge>
            {m.org === current?.org ? <span className="text-accent-text">✓</span> : null}
          </button>
        ))}
        <p className="m-0 px-2 py-2 text-caption text-muted-foreground">The server keeps the active organisation and has no route to change it, so this choice lasts as long as the tab.</p>
      </PopoverContent>
    </Popover>
  )
}
