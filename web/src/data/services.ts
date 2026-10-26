import { useQuery } from '@tanstack/react-query'
import { get, json } from '@/auth/client'
import { useOrganisations } from '@/auth/session'
import { useScope } from '@/lib/scope'
import { dur } from '@/lib/format'

export type Service = { name: string; project: string; rps: string; err: number | null; p50: string; p95: string; p99: string; seen: string; spark: number[] }

type Serving = {
  name: string
  requests: number
  errors: number
  per_second: number
  p50_us: number
  p95_us: number
  p99_us: number
  last_seen_ms: number
}

const WINDOW_MS = 60 * 60 * 1000
const FRESH_MS = 15_000

function since(ms: number): string {
  const seconds = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (seconds < 5) return 'now'
  if (seconds < 60) return `${seconds} s ago`
  if (seconds < 3600) return `${Math.round(seconds / 60)} min ago`
  return `${Math.round(seconds / 3600)} h ago`
}

export const serving = (service: Serving): Service => ({
  name: service.name,
  project: '',
  rps: `${service.per_second.toFixed(1)}/s`,
  err: service.requests === 0 ? null : (service.errors / service.requests) * 100,
  p50: dur(service.p50_us / 1000),
  p95: dur(service.p95_us / 1000),
  p99: dur(service.p99_us / 1000),
  seen: since(service.last_seen_ms),
  spark: [],
})

export function useServices(): Service[] {
  const { active } = useOrganisations()
  const { project } = useScope()
  const { data } = useQuery({
    queryKey: ['services', active?.org, project],
    enabled: Boolean(active),
    staleTime: FRESH_MS,
    queryFn: async () => {
      const to = Date.now()
      const from = to - WINDOW_MS
      const within = project ? `&project=${project}` : ''
      const found = await json<{ services: Serving[] }>(
        await get(`/api/orgs/${active?.org}/services?from=${from}&to=${to}${within}`),
      )
      return found.services.map(serving)
    },
  })
  return data ?? []
}

export function useServiceNames(windowMs: number): { names: string[]; unreachable: unknown } {
  const { active } = useOrganisations()
  const { project } = useScope()
  const { data, error } = useQuery({
    queryKey: ['service-names', active?.org, project, windowMs],
    enabled: Boolean(active),
    staleTime: FRESH_MS,
    queryFn: async () => {
      const to = Date.now()
      const within = project ? `&project=${project}` : ''
      const found = await json<{ services: Serving[] }>(
        await get(`/api/orgs/${active?.org}/services?from=${to - windowMs}&to=${to}${within}`),
      )
      return found.services.map((service) => service.name).sort()
    },
  })
  return { names: data ?? [], unreachable: error }
}
