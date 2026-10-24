import { METRICS, METRIC_SERIES } from '@/mocks/mock'
import { makeSeries } from '@/mocks/series'
import { useMock } from './seam'

export type Metric = (typeof METRICS)[number]
export type MetricSeries = { label: string; vals: number[] }

export function useMetrics(): Metric[] {
  return useMock(['metrics'], () => METRICS)
}

export function useMetricSeries(name: string): MetricSeries[] {
  return useMock(['metrics', name, 'series'], () =>
    METRIC_SERIES.map((s, si) => ({ label: s.label, vals: makeSeries(60, 31 + si + name.length, (i, r) => Math.max(0, Math.round(s.f(i) + (r() - 0.5) * 2))) })),
  )
}
