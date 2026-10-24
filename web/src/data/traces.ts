import { TRACES, traceSpans, type Span, type TraceRow } from '@/mocks/mock'
import { useMock } from './seam'

export type { Span, TraceRow }

export function useTraces(): TraceRow[] {
  return useMock(['traces'], () => TRACES)
}

export function useSpans(trace: TraceRow | undefined): Span[] {
  return useMock(['traces', trace?.id ?? 'unknown', 'spans'], () => (trace ? traceSpans(trace) : []))
}
