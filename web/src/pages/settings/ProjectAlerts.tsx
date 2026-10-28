import { Plus, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { Link } from '@/lib/router'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, NativeSelect } from '@/components/ui/input'
import { Segmented } from '@/components/ui/segmented'
import { Switch } from '@/components/ui/switch'
import { Alert, Table, Td, Th, Tr } from '@/components/ui/misc'
import { useMock } from '@/data/seam'
import { ALERT_RULES } from '@/mocks/mock'
import { cn } from '@/lib/utils'
import { H } from './parts'

export function ProjectAlerts() {
  const [rules, setRules] = useState(useMock(['organisation', 'exception-alerts'], () => ALERT_RULES))
  const [open, setOpen] = useState(false)
  const [cond, setCond] = useState('rate')
  const [thr, setThr] = useState('2')
  return (
    <>
      <H title="Exception alerts" sub="Rules on the exceptions recorded in storefront's spans and logs."><Button variant="primary" size="sm" onClick={() => setOpen(true)}><Plus />New alert</Button></H>
      <Card className="overflow-x-auto">
        <Table aria-label="Alert rules"><thead><tr><Th>Rule</Th><Th className="w-[230px]">Notify</Th><Th className="w-[130px]">Last triggered</Th><Th numeric className="w-[70px]">On</Th></tr></thead>
          <tbody>{rules.map((r, i) => (
            <Tr key={r.name}><Td className="whitespace-normal py-2"><div className="flex flex-col gap-0.5"><span className="text-control font-semibold">{r.name}</span><span className="text-caption text-muted-foreground">{r.cond}</span></div></Td><Td className="whitespace-normal text-control">{r.notify}</Td><Td className={cn('font-mono text-control', r.fired && 'text-warning-text', r.last === 'never' && 'text-muted-foreground')}>{r.last}</Td>
              <Td numeric><Switch checked={r.on} onCheckedChange={(v) => setRules((x) => x.map((y, k) => (k === i ? { ...y, on: v } : y)))} aria-label={`Enable ${r.name}`} /></Td></Tr>
          ))}</tbody></Table>
      </Card>
      <Card className="overflow-hidden">
        <div className="border-b px-4 py-3 text-control font-semibold">Recent triggers</div>
        {[['PspTimeoutError · checkout-api', 'New exception type · 412 times'], ['checkout-api error rate 4.1 %', 'Error rate above 2 %']].map(([a, b]) => (
          <div key={a} className="flex items-center gap-2.5 border-b border-border-subtle px-4 py-2.5 text-control last:border-b-0"><TriangleAlert className="size-3.5 text-warning-text" aria-hidden /><span className="flex-1 font-mono">{a}</span><span className="text-muted-foreground max-md:hidden">{b}</span><time className="font-mono text-muted-foreground">14:05</time><Link to="/services/checkout-api">Open</Link></div>
        ))}
      </Card>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent size="md" title="New exception alert" description="storefront · fires once, then again only after it resolves.">
          <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); setRules((r) => [...r, { name: cond === 'rate' ? `checkout-api error rate above ${thr} %` : 'New rule', cond: cond === 'rate' ? `Error rate of checkout-api above ${thr} % for 5 min` : 'Custom condition', notify: 'Backend · Slack #alerts-backend', last: 'never', on: true, fired: false }]); setOpen(false); toast.success('Alert created') }}>
            <Field label="When"><div><Segmented label="Condition" value={cond} onChange={setCond} options={[{ value: 'new', label: 'New exception type' }, { value: 'rate', label: 'Error rate' }, { value: 'spike', label: 'Spike' }, { value: 'silence', label: 'Silence' }]} /></div></Field>
            {cond === 'rate' ? (
              <div className="flex flex-wrap items-center gap-2 text-control">Error rate of<NativeSelect className="w-[170px] font-mono"><option>checkout-api</option><option>payments</option></NativeSelect>above<div className="flex w-20 items-center gap-1 rounded-md border border-input px-2"><input aria-label="Threshold" value={thr} onChange={(e) => setThr(e.target.value)} className="h-8 w-full bg-transparent font-mono outline-none" />%</div>for 5 min</div>
            ) : null}
            {cond === 'rate' && Number(thr) < 4.1 ? <Alert variant="warning" icon={<TriangleAlert />}>Now: 4.1 %. This rule would fire right away.</Alert> : null}
            <Field label="Notify"><NativeSelect><option>Backend team · Slack #alerts-backend, email</option><option>Payments team · Slack #alerts-payments</option></NativeSelect></Field>
            <DialogFooter><Button type="button" onClick={() => setOpen(false)}>Cancel</Button><Button variant="primary" type="submit">Create alert</Button></DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}
