import { useQuery } from '@tanstack/react-query'
import { get, json } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { nothingYet } from './orgs'

/* Decompressed payload bytes, which is the one thing this system meters. There is no count of
   spans, log records or metric points because nothing counts them, and no split by project
   because the counter is keyed by organisation. */
export type Metered = {
  ingested_bytes: number
  included_bytes: number | null
  hours: { started_at_ms: number; ingested_bytes: number }[]
}

export type Day = { at: number; bytes: number }

export function useMetered(from: number, to: number) {
  const found = useOrganizations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: ['usage', active?.org, from, to],
    enabled: Boolean(active),
    staleTime: 60_000,
    queryFn: async () => json<Metered>(await get(`/api/orgs/${active?.org}/usage?from=${from}&to=${to}`)),
  })
  return { metered: data, nothing: nothingYet(found, isPending, error, 'usage') }
}

const DAY = 86_400_000

export const startOfMonth = (at: number) => Date.UTC(new Date(at).getUTCFullYear(), new Date(at).getUTCMonth(), 1)

export const startOfNextMonth = (at: number) => Date.UTC(new Date(at).getUTCFullYear(), new Date(at).getUTCMonth() + 1, 1)

/* The store answers by the hour, because that is the grain the rollup writes. A month of hours is
   too many bars to read, so they are added into days here rather than asked for twice. Every day
   of the window gets a bar, including the ones nothing arrived in — a gap would read as a day
   that is still loading. */
export function byDay(hours: Metered['hours'], from: number, to: number): Day[] {
  const totals = new Map<number, number>()
  for (const hour of hours) {
    const day = Math.floor(hour.started_at_ms / DAY) * DAY
    totals.set(day, (totals.get(day) ?? 0) + hour.ingested_bytes)
  }
  const days: Day[] = []
  for (let at = Math.floor(from / DAY) * DAY; at < to; at += DAY) {
    days.push({ at, bytes: totals.get(at) ?? 0 })
  }
  return days
}

/* Where this month lands if the rest of it looks like the part that has happened. Said as a
   projection rather than a number, because that is what it is. */
export function projected(used: number, at: number): number {
  const start = startOfMonth(at)
  const elapsed = at - start
  const whole = startOfNextMonth(at) - start
  if (elapsed <= 0) return used
  return Math.round((used / elapsed) * whole)
}

const UNITS = ['B', 'kB', 'MB', 'GB', 'TB', 'PB']

/* Decimal rather than binary, because a plan is sold in gigabytes and a person reads this beside
   one. The allowance it is measured against is binary, which is the one place the two meet: a
   ceiling of 1 GB reads as 1.07 GB used when it is reached exactly, and saying 1.07 is better
   than quietly rescaling the number the quota refused on. */
export function bytes(count: number): string {
  if (count < 1000) return `${count} B`
  let scaled = count
  let unit = 0
  while (scaled >= 1000 && unit < UNITS.length - 1) {
    scaled /= 1000
    unit += 1
  }
  return `${scaled.toFixed(scaled < 10 ? 2 : scaled < 100 ? 1 : 0)} ${UNITS[unit]}`
}
