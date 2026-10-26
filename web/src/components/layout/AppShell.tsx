import { useQuery } from '@tanstack/react-query'
import { TriangleAlert } from 'lucide-react'
import { useMatches } from '@tanstack/react-router'
import { Button } from '@/components/ui/button'
import { Sidebar } from './Sidebar'
import { Topbar } from './Topbar'
import { MobileTabs } from './MobileTabs'
import { useGlobalShortcuts } from './useGlobalShortcuts'
import { CommandPalette, ShortcutsDialog } from './CommandPalette'
import { healthQuery } from '@/auth/api'
import { Outlet } from '@/lib/router'

function UnavailableBanner() {
  const health = useQuery(healthQuery)
  if (!health.isError) return null
  return (
    <div role="status" className="flex items-center gap-2.5 border-b border-warning-border bg-warning-subtle px-4 py-2 text-control text-warning-text">
      <TriangleAlert className="size-4 shrink-0" aria-hidden />
      <span className="flex-1"><span className="font-semibold">Backend unavailable.</span> The health check is failing, so anything on screen may be stale.</span>
      <Button variant="ghost" size="sm" className="text-inherit" loading={health.isFetching} onClick={() => void health.refetch()}>Retry now</Button>
    </div>
  )
}

export function AppShell() {
  const mock = useMatches().some((m) => m.staticData.mock === true)
  const { palette, setPalette, shortcuts, setShortcuts } = useGlobalShortcuts()
  return (
    <div className="flex min-h-screen bg-background">
      <Sidebar onOpenShortcuts={() => setShortcuts(true)} />
      <div className="flex min-w-0 flex-1 flex-col">
        <Topbar onPalette={() => setPalette(true)} mock={mock} />
        <UnavailableBanner />
        <main className="flex min-w-0 flex-1 flex-col gap-4 px-4 pb-10 pt-4 md:px-5">
          <Outlet />
        </main>
        <MobileTabs />
      </div>
      <CommandPalette open={palette} onOpenChange={setPalette} />
      <ShortcutsDialog open={shortcuts} onOpenChange={setShortcuts} />
    </div>
  )
}
