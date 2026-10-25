import { useState } from 'react'
import { Link } from '@/lib/router'
import { Card } from '@/components/ui/card'
import { Segmented } from '@/components/ui/segmented'
import { Table, Td, Th, Tr } from '@/components/ui/misc'
import { H } from './parts'

function Meter({ label, used, total, pct, note }: { label: string; used: string; total: string; pct: number; note: string }) {
  return (
    <Card className="flex flex-col gap-2 p-3.5">
      <div className="flex items-baseline gap-2"><span className="flex-1 text-control font-semibold">{label}</span><span className="font-mono text-control">{used}</span><span className="text-caption text-muted-foreground">of {total}</span></div>
      <div className="relative h-2 rounded bg-muted"><div className="absolute inset-y-0 left-0 rounded bg-primary" style={{ width: `${pct}%` }} /></div>
      <span className="text-caption text-muted-foreground">{note}</span>
    </Card>
  )
}

export function Usage() {
  const [period, setPeriod] = useState('this')
  return (
    <>
      <H title="Usage" sub="October 1 – 31 · resets in 7 days. Self-hosted shows storage only."><Segmented label="Period" value={period} onChange={setPeriod} options={[{ value: 'this', label: 'This month' }, { value: 'last', label: 'Last month' }]} /></H>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
        <Meter label="Spans" used="41.2 M" total="75 M included" pct={55} note="55 % · projected 92 % at month end" />
        <Meter label="Log records" used="118 M" total="200 M included" pct={59} note="59 % · projected 98 %" />
        <Meter label="Metric points" used="22.4 M" total="100 M included" pct={22} note="22 % · projected 36 %" />
        <Meter label="Storage" used="186 GB" total="500 GB" pct={37} note="Driven by retention: 14 days on storefront" />
      </div>
      <Card className="overflow-x-auto">
        <Table aria-label="Usage by project"><thead><tr><Th>Project</Th><Th numeric>Spans</Th><Th numeric>Log records</Th><Th numeric>Metric points</Th><Th numeric>Storage</Th><Th numeric>Share</Th></tr></thead>
          <tbody>{[['storefront', '31.9 M', '84.0 M', '16.1 M', '131 GB', '71 %'], ['payments-eu', '7.6 M', '26.3 M', '5.2 M', '41 GB', '22 %'], ['data-pipeline', '1.7 M', '7.7 M', '1.1 M', '14 GB', '7 %']].map((r) => <Tr key={r[0]}><Td className="font-semibold">{r[0]}</Td>{r.slice(1).map((c, i) => <Td key={i} numeric>{c}</Td>)}</Tr>)}</tbody></Table>
      </Card>
      <p className="m-0 text-caption text-muted-foreground">Over the included amount, Cloud bills per million; set alerts and a hard limit in <Link to="/settings/billing">Billing</Link>.</p>
    </>
  )
}
