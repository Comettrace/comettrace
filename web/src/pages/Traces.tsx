import { CircleX, Copy, Search } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate } from '@/lib/router'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { InputGroup, NativeSelect } from '@/components/ui/input'
import { Segmented } from '@/components/ui/segmented'
import { Switch } from '@/components/ui/switch'
import { Kbd, Table, Td, Th } from '@/components/ui/misc'
import { EmptyState, PageHeader, ServiceDot } from '@/components/cmt/bits'
import { useTraces } from '@/data/traces'
import { shortId } from '@/lib/format'
import { cn } from '@/lib/utils'

export function Traces() {
  const navigate = useNavigate()
  const [q, setQ] = useState('')
  const [svc, setSvc] = useState('any')
  const [errOnly, setErrOnly] = useState(false)
  const [density, setDensity] = useState<'compact' | 'comfy'>('compact')
  const [cursor, setCursor] = useState(0)
  const traces = useTraces()
  const services = [...new Set(traces.map((t) => t.svc))]
  const rows = useMemo(() => traces.filter((t) => (!errOnly || t.errors > 0) && (svc === 'any' || t.svc === svc) && t.root.toLowerCase().includes(q.toLowerCase())), [traces, q, svc, errOnly])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement
      if (el.tagName === 'INPUT' || el.tagName === 'SELECT') return
      if (e.key === 'j' || e.key === 'ArrowDown') { e.preventDefault(); setCursor((c) => Math.min(rows.length - 1, c + 1)) }
      if (e.key === 'k' || e.key === 'ArrowUp') { e.preventDefault(); setCursor((c) => Math.max(0, c - 1)) }
      if (e.key === 'Enter' && rows[cursor]) navigate(`/traces/${rows[cursor].id}`)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [rows, cursor, navigate])
  const copy = (id: string) => { void navigator.clipboard?.writeText(id); toast.success('Trace id copied', { description: shortId(id) }) }
  return (
    <>
      <PageHeader title="Traces" meta="Newest first">
        <Segmented label="Row density" value={density} onChange={setDensity} options={[{ value: 'compact', label: 'Compact' }, { value: 'comfy', label: 'Comfortable' }]} />
        <span className="hidden items-center gap-1 text-caption text-muted-foreground lg:inline-flex"><Kbd>J</Kbd><Kbd>K</Kbd>move<Kbd>↵</Kbd>open</span>
      </PageHeader>
      <div className="flex flex-wrap items-center gap-2" role="search">
        <NativeSelect aria-label="Service" value={svc} onChange={(e) => setSvc(e.target.value)} className="w-[200px]">
          <option value="any">Service: any</option>
          {services.map((s) => <option key={s} value={s}>{s}</option>)}
        </NativeSelect>
        <InputGroup className="w-[300px] max-w-full"><Search aria-hidden /><input aria-label="Span name" placeholder="Span name, e.g. POST /api/checkout" value={q} onChange={(e) => setQ(e.target.value)} /></InputGroup>
        <Switch checked={errOnly} onCheckedChange={setErrOnly} label="Errors only" />
        <Button variant="ghost" size="sm" disabled={!q && svc === 'any' && !errOnly} onClick={() => { setQ(''); setSvc('any'); setErrOnly(false) }}>Clear</Button>
      </div>
      {rows.length === 0 ? (
        <EmptyState title="No traces match" actions={<Button size="sm" onClick={() => { setQ(''); setSvc('any'); setErrOnly(false) }}>Clear filters</Button>}>Try a shorter span name, or widen the time range.</EmptyState>
      ) : (
        <div className="-mx-4 overflow-x-auto border-t md:-mx-5">
          <Table aria-label="Traces">
            <thead><tr><Th className="w-[44px]"><span className="sr-only">Status</span></Th><Th>Root span</Th><Th className="w-[150px]">Service</Th><Th className="w-[130px]">Started</Th><Th numeric className="w-[100px]">Duration</Th><Th numeric className="w-[80px]">Spans</Th><Th className="w-[170px]">Trace id</Th></tr></thead>
            <tbody>
              {rows.map((t, i) => (
                <tr key={t.id} onClick={() => navigate(`/traces/${t.id}`)} className={cn('cursor-pointer hover:[&>td]:bg-row-hover', i === cursor && '[&>td]:bg-accent-subtle')}>
                  <Td className={cn(density === 'comfy' && 'h-10')}>{t.errors ? <span className="inline-flex text-destructive-text" title={`${t.errors} errors`}><CircleX className="size-3.5" aria-label={`${t.errors} errors`} /></span> : null}</Td>
                  <Td><Link to={`/traces/${t.id}`} className="font-semibold text-foreground" onClick={(e) => e.stopPropagation()}>{t.root}</Link></Td>
                  <Td><span className="inline-flex items-center gap-1.5 text-control"><ServiceDot name={t.svc} />{t.svc}</span></Td>
                  <Td className="font-mono text-control">{t.start}</Td>
                  <Td numeric>{t.dur}</Td>
                  <Td numeric>{t.spans}</Td>
                  <Td><button type="button" onClick={(e) => { e.stopPropagation(); copy(t.id) }} className="inline-flex cursor-pointer items-center gap-1.5 rounded-sm font-mono text-caption text-muted-foreground hover:text-foreground" title={t.id} aria-label={`Copy trace id ${t.id}`}>{shortId(t.id)}<Copy className="size-3" aria-hidden /></button></Td>
                </tr>
              ))}
            </tbody>
          </Table>
          <div className="px-5 py-3"><Button size="sm">Load older</Button></div>
        </div>
      )}
    </>
  )
}
