import { useQuery } from '@tanstack/react-query'
import { get, json } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { useScope } from '@/lib/scope'
import { dur } from '@/lib/format'

export type TraceRow = { id: string; root: string; svc: string; start: string; dur: string; spans: number; errors: number }
export type Span = { id: string; name: string; svc: string; depth: number; offset: number; width: number; dur: string; err: boolean; kind: string; attrs: [string, string][] }

type Listed = {
  trace_id: string
  root_service: string
  root_name: string
  started_at_ms: number
  duration_us: number
  spans: number
  errors: number
}

type Laid = {
  span_id: string
  parent_span_id: string | null
  service: string
  name: string
  kind: string
  status: string
  start_offset_us: number
  duration_us: number
  attributes: Record<string, string>
}

type Waterfall = { trace_id: string; started_at_ms: number; duration_us: number; spans: Laid[] }

const WINDOW_MS = 24 * 60 * 60 * 1000
const FRESH_MS = 15_000

const clock = (ms: number) => new Date(ms).toTimeString().slice(0, 8) + `.${String(ms % 1000).padStart(3, '0')}`

export const listed = (trace: Listed): TraceRow => ({
  id: trace.trace_id,
  root: trace.root_name,
  svc: trace.root_service,
  start: clock(trace.started_at_ms),
  dur: dur(trace.duration_us / 1000),
  spans: trace.spans,
  errors: trace.errors,
})

/* A waterfall reads top down, so a span is placed under its parent rather than in start order, and
   the depth is how far up the chain goes. A trace missing its root, or one whose parent chain loops,
   still has to draw: the walk stops at the number of spans it has seen. */
export function laidOut(trace: Waterfall): Span[] {
  const total = trace.duration_us || 1
  const byId = new Map(trace.spans.map((span) => [span.span_id, span]))
  const children = new Map<string, Laid[]>()
  const roots: Laid[] = []
  for (const span of trace.spans) {
    const parent = span.parent_span_id
    if (parent && byId.has(parent) && parent !== span.span_id) {
      children.set(parent, [...(children.get(parent) ?? []), span])
    } else {
      roots.push(span)
    }
  }

  const out: Span[] = []
  const place = (span: Laid, depth: number) => {
    if (out.length >= trace.spans.length) return
    out.push({
      id: span.span_id,
      name: span.name,
      svc: span.service,
      depth,
      offset: (span.start_offset_us / total) * 100,
      width: Math.max((span.duration_us / total) * 100, 0.1),
      dur: dur(span.duration_us / 1000),
      err: span.status === 'error',
      kind: span.kind.toUpperCase(),
      attrs: Object.entries(span.attributes),
    })
    for (const child of children.get(span.span_id) ?? []) place(child, depth + 1)
  }
  for (const root of roots) place(root, 0)
  return out
}

export function useTraces(): TraceRow[] {
  const { active } = useOrganizations()
  const { project } = useScope()
  const { data } = useQuery({
    queryKey: ['traces', active?.org, project],
    enabled: Boolean(active),
    staleTime: FRESH_MS,
    queryFn: async () => {
      const to = Date.now()
      const from = to - WINDOW_MS
      const within = project ? `&project=${project}` : ''
      const found = await json<{ traces: Listed[] }>(
        await get(`/api/orgs/${active?.org}/traces?from=${from}&to=${to}${within}`),
      )
      return found.traces.map(listed)
    },
  })
  return data ?? []
}

export function useTrace(id: string | undefined): { row: TraceRow | undefined; spans: Span[]; pending: boolean } {
  const { active } = useOrganizations()
  const { data, isPending } = useQuery({
    queryKey: ['traces', active?.org, id],
    enabled: Boolean(active && id),
    staleTime: FRESH_MS,
    queryFn: async () => json<Waterfall>(await get(`/api/orgs/${active?.org}/traces/${id}`)),
  })
  if (!data) return { row: undefined, spans: [], pending: isPending }
  const spans = laidOut(data)
  return {
    pending: false,
    row: {
      id: data.trace_id,
      root: spans[0]?.name ?? '',
      svc: spans[0]?.svc ?? '',
      start: clock(data.started_at_ms),
      dur: dur(data.duration_us / 1000),
      spans: data.spans.length,
      errors: data.spans.filter((span) => span.status === 'error').length,
    },
    spans,
  }
}
