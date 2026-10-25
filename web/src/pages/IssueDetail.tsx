import { ChevronRight, CircleCheck, EyeOff, Github, MessageSquare, RotateCcw, ScrollText, Waypoints } from 'lucide-react'
import { useState } from 'react'
import { Link, useParams } from '@/lib/router'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardHeader, CardTitle } from '@/components/ui/card'
import { NativeSelect, Textarea } from '@/components/ui/input'
import { Avatar, Table, Td, Th, Tr } from '@/components/ui/misc'
import { Kpi, ServiceDot } from '@/components/cmt/bits'
import { LineChart } from '@/components/cmt/charts'
import { useIssueDetail, useIssues } from '@/data/issues'
import { useAssignees } from '@/data/organization'
import { NotFound } from './NotFound'
import { initials } from './Issues'
import { cn } from '@/lib/utils'

export function IssueDetail() {
  const { id } = useParams()
  const { issues, update } = useIssues()
  const members = useAssignees()
  const issue = issues.find((i) => i.id === id)
  const detail = useIssueDetail(issue ?? issues[0])
  const [tab, setTab] = useState<'details' | 'events' | 'activity'>('details')
  const [showLib, setShowLib] = useState(false)
  const [openFrame, setOpenFrame] = useState(0)
  const [comment, setComment] = useState('')
  const [added, setAdded] = useState<typeof detail.activity>([])
  if (!issue) return <NotFound />
  const activity = [...detail.activity, ...added]
  const series = detail.series
  const frames = detail.stack.filter((f) => showLib || f.inApp)
  const set = (patch: Parameters<typeof update>[1], msg: string) => { update([issue.id], patch); toast.success(msg) }
  return (
    <>
      <nav aria-label="Breadcrumb" className="text-control"><Link to="/issues">Issues</Link><span className="text-faint-foreground"> / </span><span className="font-mono">{issue.short}</span></nav>
      <div className="flex flex-wrap items-start gap-3">
        <div className="flex min-w-0 flex-[1_1_480px] flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className={cn('font-mono text-title', issue.level === 'warn' ? 'text-warning-text' : 'text-destructive-text')}>{issue.type}</h1>
            {issue.isNew ? <Badge variant="accent">New</Badge> : null}{issue.regressed ? <Badge variant="warning">Regressed</Badge> : null}
            {issue.status === 'resolved' ? <Badge variant="success"><CircleCheck />Resolved</Badge> : issue.status === 'ignored' ? <Badge variant="outline"><EyeOff />Ignored</Badge> : <Badge variant="destructive">Unresolved</Badge>}
          </div>
          <span className="text-body">{issue.message}</span>
          <span className="flex flex-wrap items-center gap-x-2 text-caption text-muted-foreground"><span className="font-mono">{issue.culprit}</span><span aria-hidden>·</span><span className="inline-flex items-center gap-1"><ServiceDot name={issue.service} />{issue.service}</span><span aria-hidden>·</span><span>{issue.platform}</span></span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {issue.status === 'unresolved' ? (
            <>
              <Button variant="primary" size="sm" onClick={() => set({ status: 'resolved', isNew: false }, 'Issue resolved')}><CircleCheck />Resolve</Button>
              <Button size="sm" onClick={() => set({ status: 'ignored' }, 'Issue ignored')}><EyeOff />Ignore</Button>
            </>
          ) : <Button size="sm" onClick={() => set({ status: 'unresolved' }, 'Issue reopened')}><RotateCcw />Reopen</Button>}
          <NativeSelect aria-label="Assignee" className="h-7 w-[170px]" value={issue.assignee ?? ''} onChange={(e) => set({ assignee: e.target.value || null }, e.target.value ? `Assigned to ${e.target.value}` : 'Unassigned')}>
            <option value="">Unassigned</option>{members.map((m) => <option key={m.email}>{m.name}</option>)}
          </NativeSelect>
          <Button variant="ghost" size="sm" onClick={() => toast.success('GitHub issue created', { description: 'acme/storefront#1284 · linked to this issue' })}><Github />Create GitHub issue</Button>
        </div>
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-3">
        <Kpi label="Events" value={issue.events.toLocaleString('en-US')} sub="last 24 h" tone={issue.isNew ? 'warn' : undefined} />
        <Kpi label="Users affected" value={issue.users.toLocaleString('en-US')} sub={issue.users ? '0.7 % of sessions' : 'no user context'} />
        <Kpi label="First seen" value={issue.first} sub={`release ${issue.release}`} />
        <Kpi label="Last seen" value={issue.last} sub={`team ${issue.team}`} />
      </div>
      <LineChart title="Events" legend="per minute, last hour" values={series} hi={24} yLabels={['24', '12', '0']} aria="Events per minute for this issue" height={110} />
      <div role="tablist" aria-label="Issue" className="flex gap-1 border-b">
        {(['details', 'events', 'activity'] as const).map((t) => (
          <button key={t} role="tab" type="button" aria-selected={tab === t} onClick={() => setTab(t)} className={cn('-mb-px inline-flex h-9 cursor-pointer items-center border-b-2 border-transparent px-2.5 text-[13.5px] font-semibold capitalize text-muted-foreground hover:text-foreground', tab === t && 'border-primary text-foreground')}>{t}{t === 'activity' ? <span className="ml-1.5 rounded-sm bg-muted px-1 font-mono text-[11px] font-medium">{activity.length}</span> : null}</button>
        ))}
      </div>

      <div className="flex flex-wrap items-start gap-4">
        <div className="flex min-w-0 flex-[1_1_560px] flex-col gap-4">
          {tab === 'details' && (
            <>
              <Card className="overflow-hidden">
                <CardHeader><CardTitle className="flex-1">Stack trace</CardTitle><label className="inline-flex cursor-pointer items-center gap-2 text-control"><input type="checkbox" checked={showLib} onChange={(e) => setShowLib(e.target.checked)} className="size-4 accent-[var(--primary)]" />Show library frames</label></CardHeader>
                <div className="border-b border-border-subtle bg-destructive-subtle px-4 py-2.5 font-mono text-control text-destructive-text">{issue.type}: {issue.message}</div>
                {frames.map((f, i) => (
                  <div key={f.file + f.fn} className="border-b border-border-subtle last:border-b-0">
                    <button type="button" aria-expanded={openFrame === i} disabled={!f.code.length} onClick={() => setOpenFrame(openFrame === i ? -1 : i)} className={cn('flex w-full items-center gap-2 px-4 py-2 text-left text-control enabled:cursor-pointer enabled:hover:bg-row-hover', !f.inApp && 'text-muted-foreground')}>
                      <ChevronRight className={cn('size-3.5 text-muted-foreground transition-transform', openFrame === i && 'rotate-90', !f.code.length && 'invisible')} aria-hidden />
                      <span className="font-mono">{f.file}</span><span className="text-muted-foreground">in</span><span className="font-mono font-semibold">{f.fn}</span><span className="text-muted-foreground">at line</span><span className="font-mono">{f.line}</span>
                      <span className="flex-1" />{f.inApp ? <Badge variant="accent" size="xs">In app</Badge> : null}
                    </button>
                    {openFrame === i && f.code.length ? (
                      <pre className="m-0 overflow-x-auto bg-sidebar py-1.5 font-mono text-[12px] leading-5">
                        {f.code.map((l, k) => (
                          <div key={k} className={cn('flex px-4', k === f.hl && 'bg-destructive-subtle text-destructive-text')}><span className="w-10 shrink-0 select-none text-right text-faint-foreground">{f.line - f.hl + k}</span><span className="pl-4">{l}</span></div>
                        ))}
                      </pre>
                    ) : null}
                  </div>
                ))}
              </Card>
              <Card className="overflow-hidden">
                <CardHeader><CardTitle className="flex-1">Tags</CardTitle><span className="text-caption text-muted-foreground">share of events</span></CardHeader>
                <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-x-6 gap-y-4 p-4">
                  {detail.tags.map((t) => (
                    <div key={t.key} className="flex flex-col gap-1.5">
                      <span className="font-mono text-caption text-muted-foreground">{t.key}</span>
                      {t.values.map(([v, p]) => (
                        <div key={v} className="flex items-center gap-2 text-control"><span className="w-[130px] truncate font-mono">{v}</span><span className="flex h-1.5 flex-1 overflow-hidden rounded bg-muted"><span className="rounded bg-chart-line" style={{ width: `${p}%` }} /></span><span className="w-10 text-right font-mono text-caption">{p} %</span></div>
                      ))}
                    </div>
                  ))}
                </div>
              </Card>
            </>
          )}
          {tab === 'events' && (
            <Card className="overflow-x-auto">
              <Table aria-label="Events"><thead><tr><Th className="w-[130px]">Time</Th><Th>Trace</Th><Th className="w-[110px]">Host</Th><Th className="w-[110px]">User</Th><Th className="w-[120px]"><span className="sr-only">Links</span></Th></tr></thead>
                <tbody>{detail.events.map(([t, tr, h, u]) => (
                  <Tr key={t}><Td className="font-mono">{t}</Td><Td><Link to={`/traces/${tr}`} className="font-mono">{tr.slice(0, 8)}…{tr.slice(-4)}</Link></Td><Td className="font-mono">{h}</Td><Td className="font-mono text-muted-foreground">{u}</Td><Td numeric><Link to="/logs" className="text-control">Logs</Link></Td></Tr>
                ))}</tbody></Table>
            </Card>
          )}
          {tab === 'activity' && (
            <Card className="flex flex-col">
              <ol className="m-0 list-none p-0">
                {activity.map((a, i) => (
                  <li key={i} className="flex items-start gap-3 border-b border-border-subtle px-4 py-3">
                    {a.who === 'Comettrace' ? <span className="inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"><MessageSquare className="size-3.5" aria-hidden /></span> : <Avatar initials={initials(a.who)} />}
                    <span className="flex-1 text-control"><span className="font-semibold">{a.who}</span> {a.what}</span>
                    <time className="font-mono text-caption text-muted-foreground">{a.when}</time>
                  </li>
                ))}
              </ol>
              <form className="flex flex-col gap-2 p-4" onSubmit={(e) => { e.preventDefault(); if (!comment.trim()) return; setAdded((a) => [...a, { who: 'You', what: `commented: "${comment.trim()}"`, when: 'now' }]); setComment('') }}>
                <Textarea aria-label="Comment" placeholder="Add a comment. Teammates on this issue get notified." value={comment} onChange={(e) => setComment(e.target.value)} className="min-h-16" />
                <div><Button variant="primary" size="sm" type="submit" disabled={!comment.trim()}>Comment</Button></div>
              </form>
            </Card>
          )}
        </div>
        <aside className="flex w-[300px] max-w-full flex-col gap-3" aria-label="Issue context">
          <Card className="flex flex-col gap-2.5 p-4 text-control">
            <h2 className="text-sub">Context</h2>
            <div className="flex justify-between gap-2"><span className="text-muted-foreground">Owner team</span><span className="font-semibold">{issue.team}</span></div>
            <div className="flex justify-between gap-2"><span className="text-muted-foreground">Assignee</span><span>{issue.assignee ?? 'Unassigned'}</span></div>
            <div className="flex justify-between gap-2"><span className="text-muted-foreground">Release</span><span className="font-mono">{issue.release}</span></div>
            <div className="flex justify-between gap-2"><span className="text-muted-foreground">Environment</span><span className="font-mono">production</span></div>
            <div className="flex justify-between gap-2"><span className="text-muted-foreground">Alert rule</span><Link to="/settings/project/alerts">New exception type</Link></div>
          </Card>
          <Card className="flex flex-col gap-2 p-4">
            <h2 className="text-sub">Jump to</h2>
            <Button asChild size="sm" className="justify-start"><Link to="/traces/4bf92f3577b34da6a3ce929d0e0e4736"><Waypoints />Latest trace</Link></Button>
            <Button asChild size="sm" className="justify-start"><Link to="/logs"><ScrollText />Logs with this exception</Link></Button>
            <Button asChild size="sm" className="justify-start"><Link to={issue.platform === 'frontend' ? '/frontend/errors' : `/services/${issue.service}`}>{issue.platform === 'frontend' ? 'Frontend errors' : `Service ${issue.service}`}</Link></Button>
          </Card>
          <p className="m-0 text-caption text-muted-foreground">Resolving marks it fixed in the next release. If the same error comes back after that, the issue reopens as Regressed and alerts again.</p>
        </aside>
      </div>
    </>
  )
}
