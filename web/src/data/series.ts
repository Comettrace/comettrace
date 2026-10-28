import { makeSeries } from '@/mocks/series'
import { useMock } from './seam'

export const TIME_LABELS = ['13:32', '13:47', '14:02', '14:17', '14:32']

export const DEPLOY_MARKERS = [{ at: 32, label: '2.14.1', to: '/releases/2.14.1' }]

export type Series = { spans: number[]; errorSpans: number[]; errorLogs: number[] }

export function useOverviewSeries(): Series {
  return useMock(['series', 'overview'], () => ({
    spans: makeSeries(60, 11, (i, r) => 1750 + Math.sin(i / 7) * 120 + (r() - 0.5) * 90),
    errorSpans: makeSeries(60, 12, (i, r) => (i >= 33 ? 17 + (r() - 0.4) * 6 : 2 + r() * 2)),
    errorLogs: makeSeries(60, 13, (i, r) => (i >= 33 ? 38 + (r() - 0.4) * 10 : 6 + r() * 4)),
  }))
}

export function useServiceSeries(name: string, degraded: boolean) {
  return useMock(['series', 'service', name, String(degraded)], () => ({
    requests: makeSeries(60, 21, (i, r) => 40 + Math.sin(i / 6) * 4 + (r() - 0.5) * 5),
    errorRate: makeSeries(60, 22, (i, r) => (degraded && i >= 33 ? 3.4 + (r() - 0.3) * 1.6 : 0.3 + r() * 0.3)),
    p95: makeSeries(60, 23, (i, r) => (degraded && i >= 33 ? 1150 + r() * 380 : 380 + r() * 90)),
    p50: makeSeries(60, 24, (i, r) => (degraded && i >= 33 ? 110 + r() * 20 : 80 + r() * 12)),
  }))
}
