import { Bug, CircleCheck, EyeOff, Search, Server, Monitor } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Link, useNavigate } from '@/lib/router'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { InputGroup, NativeSelect } from '@/components/ui/input'
import { Segmented } from '@/components/ui/segmented'
import { Avatar, Table, Td, Th } from '@/components/ui/misc'
import { EmptyState, PageHeader } from '@/components/cmt/bits'
import { Sparkline } from '@/components/cmt/charts'
import { useAssignees } from '@/data/organization'
import { useIssues, type IssueStatus } from '@/data/issues'
import { cn } from '@/lib/utils'

export const initials = (n: string) => n.split(' ').map((x) => x[0]).join('').slice(0, 2)

export function Issues() {
  const { issues, update } = useIssues()
  const members = useAssignees()
  const navigate = useNavigate()
  const [tab, setTab] = useState<IssueStatus | 'review'>('unresolved')
  const [q, setQ] = useState('')
  const [svc, setSvc] = useState('any')
  const [platform, setPlatform] = useState<'all' | 'backend' | 'frontend'>('all')
  const [sort, setSort] = useState<'last' | 'events' | 'users' | 'new'>('last')
  const [sel, setSel] = useState<string[]>([])
  const count = (s: IssueStatus) => issues.filter((i) => i.status === s).length
  const review = issues.filter((i) => i.status === 'unresolved' && (i.isNew || i.regressed) && !i.assignee).length
  const rows = useMemo(() => {
    let r = issues.filter((i) => (tab === 'review' ? i.status === 'unresolved' && (i.isNew || i.regressed) && !i.assignee : i.status === tab))
    r = r.filter((i) => (svc === 'any' || i.service === svc) && (platform === 'all' || i.platform === platform) && (i.type + i.message + i.culprit + i.short).toLowerCase().includes(q.toLowerCase()))
    if (sort === 'events') r = [...r].sort((a, b) => b.events - a.events)
    if (sort === 'users') r = [...r].sort((a, b) => b.users - a.users)
    if (sort === 'new') r = [...r].sort((a, b) => Number(!!b.isNew) - Number(!!a.isNew))
    return r
  }, [issues, tab, q, svc, platform, sort])
  const services = [...new Set(issues.map((i) => i.service))]
  const allSel = rows.length > 0 && rows.every((r) => sel.includes(r.id))
  const bulk = (patch: Parameters<typeof update>[1], msg: string) => { update(sel, patch); toast.success(msg, { description: `${sel.length} issue${sel.length > 1 ? 's' : ''}` }); setSel([]) }
  return (
    <>
      <PageHeader title="Issues" meta="Exceptions from spans and logs, grouped by type and top frame" />
      <div role="tablist" aria-label="Issue status" className="flex gap-1 overflow-x-auto border-b">
        {([['unresolved', 'Unresolved', count('unresolved')], ['review', 'For review', review], ['ignored', 'Ignored', count('ignored')], ['resolved', 'Resolved', count('resolved')]] as const).map(([v, l, n]) => (
          <button key={v} role="tab" type="button" aria-selected={tab === v} onClick={() => { setTab(v); setSel([]) }} className={cn('-mb-px inline-flex h-9 cursor-pointer items-center gap-1.5 whitespace-nowrap border-b-2 border-transparent px-2.5 text-[13.5px] font-semibold text-muted-foreground hover:text-foreground', tab === v && 'border-primary text-foreground')}>
            {l}<span className="rounded-sm bg-muted px-1 font-mono text-[11px] font-medium leading-4">{n}</span>
          </button>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-2" role="search">
        <InputGroup className="w-[300px] max-w-full"><Search aria-hidden /><input aria-label="Search issues" placeholder="Search type, message, file or id" value={q} onChange={(e) => setQ(e.target.value)} /></InputGroup>
        <NativeSelect aria-label="Service" value={svc} onChange={(e) => setSvc(e.target.value)} className="w-[190px]"><option value="any">Service: any</option>{services.map((s) => <option key={s}>{s}</option>)}</NativeSelect>
        <Segmented label="Platform" value={platform} onChange={setPlatform} options={[{ value: 'all', label: 'All' }, { value: 'backend', label: <><Server className="size-3.5" />Backend</> }, { value: 'frontend', label: <><Monitor className="size-3.5" />Frontend</> }]} />
        <span className="flex-1" />
        <NativeSelect aria-label="Sort" value={sort} onChange={(e) => setSort(e.target.value as typeof sort)} className="w-[170px]"><option value="last">Sort: last seen</option><option value="events">Sort: events</option><option value="users">Sort: users</option><option value="new">Sort: new first</option></NativeSelect>
      </div>
      {sel.length ? (
        <div className="flex flex-wrap items-center gap-2 rounded-md border bg-accent-subtle px-3 py-2 text-control" role="toolbar" aria-label="Bulk actions">
          <span className="font-semibold text-accent-text">{sel.length} selected</span><span className="flex-1" />
          <Button size="sm" onClick={() => bulk({ status: 'resolved' }, 'Resolved')}><CircleCheck />Resolve</Button>
          <Button size="sm" onClick={() => bulk({ status: 'ignored' }, 'Ignored')}><EyeOff />Ignore</Button>
          <NativeSelect aria-label="Assign selected" className="h-7 w-[170px]" value="" onChange={(e) => e.target.value && bulk({ assignee: e.target.value }, `Assigned to ${e.target.value}`)}><option value="">Assign to…</option>{members.map((m) => <option key={m.email}>{m.name}</option>)}</NativeSelect>
          <Button variant="ghost" size="sm" onClick={() => setSel([])}>Clear</Button>
        </div>
      ) : null}
      {rows.length === 0 ? (
        <EmptyState icon={<Bug />} title={tab === 'unresolved' ? 'No unresolved issues' : 'Nothing here'}>{q || svc !== 'any' || platform !== 'all' ? 'No issue matches these filters.' : 'New exceptions show up here within a minute of happening.'}</EmptyState>
      ) : (
        <div className="-mx-4 overflow-x-auto border-t md:-mx-5">
          <Table aria-label="Issues">
            <thead><tr>
              <Th className="w-[40px]"><input type="checkbox" aria-label="Select all" checked={allSel} onChange={() => setSel(allSel ? [] : rows.map((r) => r.id))} className="size-4 accent-[var(--primary)]" /></Th>
              <Th>Issue</Th><Th className="w-[120px]">Last 24 h</Th><Th numeric className="w-[90px]">Events</Th><Th numeric className="w-[80px]">Users</Th><Th className="w-[130px]">Last seen</Th><Th className="w-[70px]">Owner</Th>
            </tr></thead>
            <tbody>
              {rows.map((i) => (
                <tr key={i.id} onClick={() => navigate(`/issues/${i.id}`)} className={cn('cursor-pointer hover:[&>td]:bg-row-hover', sel.includes(i.id) && '[&>td]:bg-accent-subtle')}>
                  <Td onClick={(e) => e.stopPropagation()}><input type="checkbox" aria-label={`Select ${i.short}`} checked={sel.includes(i.id)} onChange={() => setSel((s) => (s.includes(i.id) ? s.filter((x) => x !== i.id) : [...s, i.id]))} className="size-4 accent-[var(--primary)]" /></Td>
                  <Td className="whitespace-normal py-2">
                    <div className="flex min-w-0 flex-col gap-0.5">
                      <span className="flex flex-wrap items-center gap-2">
                        <Link to={`/issues/${i.id}`} onClick={(e) => e.stopPropagation()} className={cn('font-mono font-semibold', i.level === 'warn' ? 'text-warning-text' : 'text-destructive-text')}>{i.type}</Link>
                        {i.isNew ? <Badge variant="accent" size="xs">New</Badge> : null}{i.regressed ? <Badge variant="warning" size="xs">Regressed</Badge> : null}{i.level === 'fatal' ? <Badge variant="destructive" size="xs">Fatal</Badge> : null}
                      </span>
                      <span className="truncate text-control">{i.message}</span>
                      <span className="flex flex-wrap gap-x-2 text-caption text-muted-foreground"><span className="font-mono">{i.short}</span><span aria-hidden>·</span><span className="font-mono">{i.culprit}</span><span aria-hidden>·</span><span>{i.service}</span><span aria-hidden>·</span><span>{i.platform}</span><span aria-hidden>·</span><span>first {i.first}</span></span>
                    </div>
                  </Td>
                  <Td><Sparkline values={i.spark} /></Td>
                  <Td numeric>{i.events.toLocaleString('en-US')}</Td>
                  <Td numeric>{i.users.toLocaleString('en-US')}</Td>
                  <Td className="font-mono text-control">{i.last}</Td>
                  <Td>{i.assignee ? <span title={i.assignee}><Avatar initials={initials(i.assignee)} className="size-6 text-[10px]" /></span> : <span className="text-caption text-faint-foreground">—</span>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          <p className="m-0 px-5 py-3 text-caption text-muted-foreground">An issue groups events with the same exception type and top in-app frame. A resolved issue that comes back is marked Regressed.</p>
        </div>
      )}
    </>
  )
}
