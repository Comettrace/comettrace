import { RELEASES, type Release } from '@/mocks/releases'
import { makeSeries } from '@/mocks/series'
import { useMock } from './seam'

export type { Release }

export function useReleases(): Release[] {
  return useMock(['releases'], () => RELEASES)
}

export function useRelease(version: string | undefined): Release | undefined {
  return useReleases().find((r) => r.version === version)
}

export function useReleaseSeries(release: Release | undefined, degraded: boolean) {
  const at = release?.markerAt ?? 30
  return useMock(['releases', release?.version ?? 'unknown', 'series', String(degraded)], () => ({
    at,
    errorRate: makeSeries(60, 70, (i, r) => (degraded && i >= at ? 3.4 + (r() - 0.3) * 1.6 : 0.3 + r() * 0.3)),
    p95: makeSeries(60, 71, (i, r) => (degraded && i >= at ? 1150 + r() * 380 : 380 + r() * 90)),
  }))
}
