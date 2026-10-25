import { useMemo, useState } from 'react'
import { Card } from '@/components/ui/card'
import { Segmented } from '@/components/ui/segmented'
import { Table, Td, Th, Tr } from '@/components/ui/misc'
import { byDay, bytes, projected, startOfMonth, startOfNextMonth, useMetered, type Day } from '@/data/usage'
import { Link } from '@/lib/router'
import { cn } from '@/lib/utils'
import { H, Sub } from './parts'

const DAY = 86_400_000

const month = (at: number) => new Date(at).toLocaleDateString('en-US', { month: 'long', year: 'numeric', timeZone: 'UTC' })
const dayLabel = (at: number) => new Date(at).toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })

/* A headline against a ceiling is a stat tile, not a chart: one number, one comparison, no axis
   worth drawing. The bar is the comparison, and the number is read off the text beside it. */
function Meter({ used, included }: { used: number; included: number | null }) {
  const share = included && included > 0 ? Math.min(100, (used / included) * 100) : null
  return (
    <Card className="flex flex-col gap-2 p-3.5">
      <div className="flex items-baseline gap-2">
        <span className="flex-1 text-control font-semibold">Ingested this month</span>
        <span className="font-mono text-control">{bytes(used)}</span>
        {included ? <span className="text-caption text-muted-foreground">of {bytes(included)} included</span> : null}
      </div>
      {share === null ? (
        <span className="text-caption text-muted-foreground">This plan sets no ceiling, so there is nothing to fill.</span>
      ) : (
        <>
          <div className="relative h-2 rounded bg-muted" role="img" aria-label={`${share.toFixed(0)} percent of the included amount`}>
            <div className="absolute inset-y-0 left-0 rounded bg-series" style={{ width: `${share}%` }} />
          </div>
          <span className="text-caption text-muted-foreground">{share.toFixed(0)} % used</span>
        </>
      )}
    </Card>
  )
}

function Stat({ label, value, note }: { label: string; value: string; note: string }) {
  return (
    <Card className="flex flex-col gap-1 p-3.5">
      <span className="text-control font-semibold">{label}</span>
      <span className="font-mono text-title">{value}</span>
      <span className="text-caption text-muted-foreground">{note}</span>
    </Card>
  )
}

/* One series of daily totals: bars, because the buckets are discrete and the question is
   magnitude per day. No legend — the heading names the one series — and no number on every bar.
   The two ends of the window are labelled and the rest is read by hovering.

   The readout sits in the heading rather than floating over the hovered bar. A bubble centred on
   a bar overflows the card at either end of a month, and on a narrow screen that is the page
   itself scrolling sideways; one fixed place cannot. The title attribute stays for a pointer that
   is really a finger, and the table underneath is how every value is read at once. */
function Daily({ days }: { days: Day[] }) {
  const hi = Math.max(1, ...days.map((d) => d.bytes))
  const [hovered, setHovered] = useState<Day | null>(null)
  return (
    <Card className="flex flex-col gap-2.5 p-3.5">
      <div className="flex items-baseline gap-2">
        <span className="flex-1 text-control font-semibold">Ingested per day</span>
        {hovered ? (
          <span className="text-control"><span className="font-semibold">{dayLabel(hovered.at)}</span> <span className="font-mono">{bytes(hovered.bytes)}</span></span>
        ) : (
          <span className="text-caption text-muted-foreground">peak {bytes(hi)}</span>
        )}
      </div>
      <div
        className="flex h-[120px] items-end gap-[2px]"
        role="img"
        aria-label={`Bytes ingested per day, ${days.length} days, peak ${bytes(hi)}`}
        onMouseLeave={() => setHovered(null)}
      >
        {days.map((d) => (
          <div
            key={d.at}
            className="flex h-full flex-1 items-end"
            title={`${dayLabel(d.at)} · ${bytes(d.bytes)}`}
            onMouseEnter={() => setHovered(d)}
          >
            <div
              className={cn('w-full rounded-t-[4px] bg-series', d.bytes === 0 && 'bg-border')}
              style={{ height: `${Math.max(d.bytes === 0 ? 1 : 2, (d.bytes / hi) * 100)}%` }}
            />
          </div>
        ))}
      </div>
      <div className="flex justify-between font-mono text-caption text-faint-foreground">
        <span>{days.length > 0 ? dayLabel(days[0].at) : ''}</span>
        <span>{days.length > 0 ? dayLabel(days[days.length - 1].at) : ''}</span>
      </div>
    </Card>
  )
}

export function Usage() {
  const [period, setPeriod] = useState('this')
  const now = Date.now()

  const [from, to] = useMemo(() => {
    const start = startOfMonth(now)
    return period === 'this'
      ? [start, startOfNextMonth(now)]
      : [startOfMonth(start - DAY), start]
  }, [period, now])

  const { metered, nothing } = useMetered(from, to)
  const days = useMemo(() => (metered ? byDay(metered.hours, from, Math.min(to, now + DAY)) : []), [metered, from, to, now])
  const rolled = days.reduce((total, d) => total + d.bytes, 0)
  /* This month's figure is the live counter, which counts the hour in progress. A past month has
     no live counter to read, so it is the rolled up hours and nothing else. */
  const used = period === 'this' ? (metered?.ingested_bytes ?? 0) : rolled

  return (
    <>
      <H title="Usage" sub={`${month(from)} · decompressed payload bytes, per organization.`}>
        <Segmented
          label="Period"
          value={period}
          onChange={setPeriod}
          options={[{ value: 'this', label: 'This month' }, { value: 'last', label: 'Last month' }]}
        />
      </H>
      {metered ? (
        <>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
            <Meter used={used} included={metered.included_bytes} />
            {period === 'this' ? (
              <Stat label="Projected at month end" value={bytes(projected(used, now))} note="If the rest of the month looks like the part that has happened." />
            ) : (
              <Stat label="Rolled up" value={bytes(rolled)} note="Every finished hour of that month, added up." />
            )}
            <Stat label="Busiest day" value={bytes(Math.max(0, ...days.map((d) => d.bytes)))} note={days.length > 0 ? `Of ${days.length} days read.` : 'Nothing has arrived yet.'} />
          </div>
          <Daily days={days} />
          <Sub>By day</Sub>
          <Card className="max-h-[320px] overflow-auto">
            <Table aria-label="Ingested per day">
              <thead><tr><Th>Day</Th><Th numeric className="w-[160px]">Ingested</Th><Th numeric className="w-[120px]">Share</Th></tr></thead>
              <tbody>{[...days].reverse().map((d) => (
                <Tr key={d.at}>
                  <Td className="font-mono">{dayLabel(d.at)}</Td>
                  <Td numeric className="font-mono">{bytes(d.bytes)}</Td>
                  <Td numeric className="text-muted-foreground">{rolled > 0 ? `${((d.bytes / rolled) * 100).toFixed(1)} %` : '—'}</Td>
                </Tr>
              ))}</tbody>
            </Table>
            {days.length === 0 ? <p className="m-0 px-4 py-3 text-control text-muted-foreground">No finished hour of this month has been rolled up yet.</p> : null}
          </Card>
        </>
      ) : (
        <Card className="px-4 py-3 text-control text-muted-foreground">{nothing}</Card>
      )}
      <p className="m-0 text-caption text-muted-foreground">
        What is metered is the decompressed size of each payload accepted, counted per organization — not per project, because the counter is keyed by the organization. Nothing counts spans, log records or metric points, and nothing measures what the span store holds. The ceiling is the <Link to="/settings/billing">plan</Link> the control plane carries, and it is the same number a payload is refused against.
      </p>
    </>
  )
}
