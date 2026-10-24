import { useQuery } from '@tanstack/react-query'
import { Bell, Building2, ChevronDown, ChevronsUpDown, Clock, Folder, Keyboard, LogOut, Moon, RefreshCw, Search, Settings2, TriangleAlert, UserRound } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useMatches } from '@tanstack/react-router'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Segmented } from '@/components/ui/segmented'
import { Kbd, Avatar } from '@/components/ui/misc'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { NAV, ALL_NAV } from './nav'
import { CommandPalette, ShortcutsDialog } from './CommandPalette'
import { NotificationsPanel } from './Notifications'
import { useNotifications } from '@/data/notifications'
import { useIssues } from '@/data/issues'
import { useAlertRules } from '@/data/alerts'
import { useProjects } from '@/data/organization'
import { healthQuery } from '@/auth/api'
import { useAccount, useOrganizations, useSignOut } from '@/auth/session'
import { useTheme, type ThemePref } from '@/lib/theme'
import { Link, NavLink, Outlet, isActivePath, useLocation, useNavigate } from '@/lib/router'
import { cn } from '@/lib/utils'
import { Logo } from '@/components/cmt/Logo'
import { MockData } from '@/components/cmt/MockData'

const orgLabel = (org: string | undefined) => (org ? `org-${org.slice(0, 8)}` : 'no organisation')

function Sidebar({ onOpenShortcuts }: { onOpenShortcuts: () => void }) {
  const { pathname } = useLocation()
  const [open, setOpen] = useState<Record<string, boolean>>({})
  const notifs = useNotifications()
  const { issues } = useIssues()
  const rules = useAlertRules()
  const account = useAccount()
  const { active } = useOrganizations()
  const signingOut = useSignOut()
  const newIssues = issues.filter((x) => x.status === 'unresolved' && x.isNew).length
  const firing = rules.filter((r) => r.firing && r.on).length
  const { pref, setPref } = useTheme()
  const navigate = useNavigate()
  const initials = (account?.account ?? '??').slice(0, 2).toUpperCase()
  return (
    <aside className="sticky top-0 hidden h-screen w-[224px] shrink-0 flex-col border-r bg-sidebar md:flex" aria-label="Primary">
      <Link to="/" className="flex h-12 shrink-0 items-center gap-2 border-b px-4 text-[15px] font-bold text-foreground no-underline">
        <Logo className="size-5" /> Comettrace
      </Link>
      <nav aria-label="Main" className="scrollbar-none flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
        {NAV.map((g) => {
          const inGroup = g.items.some((i) => isActivePath(pathname, i.to))
          const expanded = !g.collapsible || (open[g.label!] ?? inGroup)
          return (
            <div key={g.label ?? 'top'} className="flex flex-col gap-0.5">
              {g.label && g.collapsible ? (
                <button
                  type="button"
                  aria-expanded={expanded}
                  onClick={() => setOpen((o) => ({ ...o, [g.label!]: !expanded }))}
                  className="flex w-full cursor-pointer items-center rounded-sm px-2.5 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-[.05em] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                >
                  <span className="flex-1 text-left">{g.label}</span>
                  {!expanded ? <span className="mr-1.5 font-mono text-[10.5px] font-medium normal-case tracking-normal">{g.items.length}</span> : null}
                  <ChevronDown className={cn('size-3 transition-transform', !expanded && '-rotate-90')} aria-hidden />
                </button>
              ) : g.label ? (
                <span className="px-2.5 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-[.05em] text-muted-foreground">{g.label}</span>
              ) : null}
              {expanded &&
                g.items.map((i) => (
                  <NavLink
                    key={i.to}
                    to={i.to}
                    end={i.to === '/'}
                    className={({ isActive: a }) =>
                      cn('flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[13.5px] font-semibold text-muted-foreground no-underline hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring', a && 'bg-muted text-foreground')
                    }
                  >
                    <i.icon className="size-4 shrink-0" aria-hidden />
                    <span className="flex-1 truncate">{i.label}</span>
                    {i.badge && i.to === '/issues' && newIssues ? <span className="rounded-full bg-destructive-subtle px-1.5 font-mono text-[11px] font-semibold leading-[18px] text-destructive-text" aria-label={`${newIssues} new issues`}>{newIssues} new</span> : null}
                    {i.badge && i.to === '/alerts' && firing ? <span className="rounded-full bg-destructive-subtle px-1.5 font-mono text-[11px] font-semibold leading-[18px] text-destructive-text" aria-label={`${firing} alerts firing`}>{firing} firing</span> : null}
                    {!i.badge && i.keys ? <span className="flex gap-0.5"><Kbd>G</Kbd><Kbd>{i.keys}</Kbd></span> : null}
                  </NavLink>
                ))}
            </div>
          )
        })}
      </nav>
      <div className="flex flex-col gap-0.5 border-t p-2">
        <Popover>
          <PopoverTrigger asChild>
            <button type="button" aria-label={`Notifications, ${notifs.unread} unread`} className="sb-row data-[state=open]:bg-muted">
              <span className="relative inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                <Bell className="size-3.5" aria-hidden />
                {notifs.unread ? <span className="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-primary ring-2 ring-sidebar" /> : null}
              </span>
              <span className="flex min-w-0 flex-1 flex-col leading-4">
                <span className="truncate text-control font-semibold">Notifications</span>
                <span className="truncate text-caption text-muted-foreground">{notifs.items.find((n) => n.unread)?.title ?? 'All caught up'}</span>
              </span>
              {notifs.unread ? <span className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-primary px-1.5 font-mono text-[11px] font-semibold text-primary-foreground">{notifs.unread}</span> : null}
            </button>
          </PopoverTrigger>
          <PopoverContent side="right" align="end" className="w-[400px] overflow-hidden p-0">
            <NotificationsPanel />
          </PopoverContent>
        </Popover>
        <NavLink to="/settings" className={({ isActive: a }) => cn('sb-row no-underline', a && 'bg-muted')}>
          <span className="inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"><Settings2 className="size-3.5" aria-hidden /></span>
          <span className="flex min-w-0 flex-1 flex-col leading-4">
            <span className="truncate text-control font-semibold">Settings</span>
            <span className="truncate text-caption text-muted-foreground">Personal, organization, project</span>
          </span>
          <span className="flex gap-0.5"><Kbd>G</Kbd><Kbd>S</Kbd></span>
        </NavLink>
        <Popover>
          <PopoverTrigger asChild>
            <button type="button" aria-label="Account menu" className="sb-row data-[state=open]:bg-muted">
              <Avatar initials={initials} />
              <span className="flex min-w-0 flex-1 flex-col leading-4">
                <span className="truncate text-control font-semibold">Signed in</span>
                <span className="truncate font-mono text-caption text-muted-foreground">{account?.account.slice(0, 13) ?? '—'}</span>
              </span>
              <ChevronsUpDown className="size-3.5 text-muted-foreground" aria-hidden />
            </button>
          </PopoverTrigger>
          <PopoverContent side="top" align="start" className="w-[264px] p-1">
            <div className="mb-1 flex items-center gap-2.5 border-b px-2 pb-2.5 pt-2">
              <Avatar initials={initials} />
              <div className="flex min-w-0 flex-1 flex-col leading-[17px]">
                <span className="truncate text-control font-semibold">Signed in</span>
                <span className="truncate font-mono text-caption text-muted-foreground">{account?.account.slice(0, 13) ?? '—'}</span>
              </div>
              {active ? <Badge variant="accent">{active.role}</Badge> : null}
            </div>
            <MenuItem icon={<UserRound />} onClick={() => navigate('/settings/profile')}>Profile</MenuItem>
            <div className="flex h-[30px] items-center gap-2 px-2 text-control">
              <Moon className="size-3.5" aria-hidden />Theme
              <span className="ml-auto"><Segmented size="sm" label="Theme" value={pref} onChange={(v: ThemePref) => setPref(v)} options={[{ value: 'system', label: 'System' }, { value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }]} /></span>
            </div>
            <MenuItem icon={<Keyboard />} onClick={onOpenShortcuts} hint={<Kbd>?</Kbd>}>Keyboard shortcuts</MenuItem>
            <div className="my-1 h-px bg-border" />
            <MenuItem icon={<LogOut />} onClick={() => signingOut.mutate()}>Sign out</MenuItem>
          </PopoverContent>
        </Popover>
      </div>
    </aside>
  )
}

function MenuItem({ icon, children, hint, onClick }: { icon: React.ReactNode; children: React.ReactNode; hint?: React.ReactNode; onClick?: () => void }) {
  return (
    <button type="button" onClick={onClick} className="flex h-[30px] w-full cursor-pointer items-center gap-2 rounded-sm px-2 text-left text-control text-foreground hover:bg-muted focus-visible:bg-muted focus-visible:outline-none [&>svg]:size-3.5">
      {icon}
      <span className="flex-1">{children}</span>
      {hint}
    </button>
  )
}

const RANGES = ['Last 15 m', 'Last 1 h', 'Last 24 h', 'Last 7 d'] as const

function OrganizationSwitcher() {
  const { list, active } = useOrganizations()
  const [chosen, setChosen] = useState<string | null>(null)
  const current = list.find((m) => m.org === chosen) ?? active
  if (list.length < 2) return <span className="hidden whitespace-nowrap font-mono font-semibold sm:inline">{orgLabel(active?.org)}</span>
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" aria-haspopup="listbox"><Building2 /><span className="font-mono">{orgLabel(current?.org)}</span><ChevronDown className="text-muted-foreground" /></Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[300px] p-1">
        {list.map((m) => (
          <button key={m.org} type="button" onClick={() => setChosen(m.org)} className="flex h-[30px] w-full cursor-pointer items-center gap-2 rounded-sm px-2 text-left text-control hover:bg-muted">
            <span className="flex-1 truncate font-mono">{orgLabel(m.org)}</span>
            <Badge variant="outline">{m.role}</Badge>
            {m.org === current?.org ? <span className="text-accent-text">✓</span> : null}
          </button>
        ))}
        <p className="m-0 px-2 py-2 text-caption text-muted-foreground">The server keeps the active organisation and has no route to change it, so this choice lasts as long as the tab.</p>
      </PopoverContent>
    </Popover>
  )
}

function Topbar({ onPalette, mock }: { onPalette: () => void; mock: boolean }) {
  const projects = useProjects()
  const [project, setProject] = useState('All projects')
  const [range, setRange] = useState<string>('Last 1 h')
  return (
    <header className="sticky top-0 z-20 flex h-12 shrink-0 items-center gap-2 border-b bg-background px-4">
      <Link to="/" className="text-foreground md:hidden" aria-label="Comettrace home"><Logo className="size-5" /></Link>
      <span className="hidden text-caption text-muted-foreground sm:inline">Organisation</span>
      <OrganizationSwitcher />
      <span className="hidden text-faint-foreground sm:inline" aria-hidden>/</span>
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="sm" aria-haspopup="listbox"><Folder />{project}<ChevronDown className="text-muted-foreground" /></Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-[260px] p-1">
          {['All projects', ...projects.map((p) => p.name)].map((p) => (
            <button key={p} type="button" onClick={() => setProject(p)} className="flex h-[30px] w-full cursor-pointer items-center rounded-sm px-2 text-left text-control hover:bg-muted">
              {p}{p === project ? <span className="ml-auto text-accent-text">✓</span> : null}
            </button>
          ))}
          <p className="m-0 flex items-center gap-2 px-2 py-2 text-caption text-muted-foreground"><MockData />no project route exists yet</p>
        </PopoverContent>
      </Popover>
      {mock ? <MockData className="max-sm:hidden" /> : null}
      <span className="flex-1" />
      <button type="button" onClick={onPalette} aria-label="Search" className="hidden h-8 w-[260px] cursor-pointer items-center gap-2 rounded-md border border-input bg-background px-2.5 text-control text-faint-foreground shadow-sm hover:bg-muted lg:flex">
        <Search className="size-3.5" aria-hidden /><span className="flex-1 text-left">Search or paste an id</span><Kbd>/</Kbd><Kbd>⌘</Kbd><Kbd>K</Kbd>
      </button>
      <Popover>
        <PopoverTrigger asChild>
          <Button size="sm"><Clock />{range}<span className="hidden font-mono text-[11px] text-faint-foreground sm:inline">→ now</span><ChevronDown className="text-muted-foreground" /></Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-[200px] p-1">
          {RANGES.map((r, i) => (
            <button key={r} type="button" onClick={() => setRange(r)} className="flex h-[30px] w-full cursor-pointer items-center rounded-sm px-2 text-left text-control hover:bg-muted">
              {r}<span className="ml-auto">{r === range ? <span className="text-accent-text">✓</span> : <Kbd>{i + 1}</Kbd>}</span>
            </button>
          ))}
        </PopoverContent>
      </Popover>
      <Button variant="ghost" size="icon-sm" aria-label="Refresh"><RefreshCw /></Button>
    </header>
  )
}

function MobileTabs() {
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
  const [palette, setPalette] = useState(false)
  const [shortcuts, setShortcuts] = useState(false)
  const navigate = useNavigate()
  const mock = useMatches().some((m) => m.staticData.mock === true)
  const goKeys = useMemo(() => Object.fromEntries([...ALL_NAV.filter((i) => i.keys).map((i) => [i.keys!.toLowerCase(), i.to]), ['s', '/settings']]), [])
  useEffect(() => {
    let g = false
    let t: number | undefined
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement
      if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable)) return
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette(true); return }
      if (e.key === '/' && !e.metaKey && !e.ctrlKey) { e.preventDefault(); setPalette(true); return }
      if (e.key === '?') { setShortcuts(true); return }
      if (g) {
        const to = goKeys[e.key.toLowerCase()]
        g = false
        if (to) { e.preventDefault(); navigate(to) }
        return
      }
      if (e.key.toLowerCase() === 'g' && !e.metaKey && !e.ctrlKey) { g = true; window.clearTimeout(t); t = window.setTimeout(() => (g = false), 1200) }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [goKeys, navigate])
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
