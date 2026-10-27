import { queryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { del, get, json } from '@/auth/client'
import { stamp } from '@/lib/format'

export type LiveSession = {
  id: string
  from: string | null
  created_at_ms: number
  last_seen_at_ms: number
  current: boolean
}

const BROWSERS: [RegExp, string][] = [
  [/Edg\//, 'Edge'],
  [/OPR\/|Opera/, 'Opera'],
  [/Firefox\//, 'Firefox'],
  [/Chrome\//, 'Chrome'],
  [/Safari\//, 'Safari'],
  [/curl\//, 'curl'],
]

const SYSTEMS: [RegExp, string][] = [
  [/Windows NT/, 'Windows'],
  [/iPhone|iPad/, 'iOS'],
  [/Mac OS X/, 'macOS'],
  [/Android/, 'Android'],
  [/Linux/, 'Linux'],
]

const UNNAMED = 'An unnamed device'

export function deviceOf(agent: string | null) {
  if (!agent) return UNNAMED
  const browser = BROWSERS.find(([pattern]) => pattern.test(agent))?.[1]
  const system = SYSTEMS.find(([pattern]) => pattern.test(agent))?.[1]
  if (browser && system) return `${browser} on ${system}`
  return browser ?? system ?? UNNAMED
}

export function ago(ms: number) {
  const seconds = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (seconds < 60) return 'now'
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  return stamp(ms)
}

export const sessionsQuery = queryOptions({
  queryKey: ['auth', 'sessions'],
  queryFn: async () => json<LiveSession[]>(await get('/auth/sessions')),
  retry: false,
  staleTime: 30_000,
})

export function useSessions() {
  const { data, isPending, error } = useQuery(sessionsQuery)
  return { sessions: data, asking: isPending, error }
}

export function useEndSession() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (id: string) => {
      await del(`/auth/sessions/${id}`)
    },
    onSuccess: () => client.invalidateQueries({ queryKey: sessionsQuery.queryKey }),
  })
}

export function useEndOtherSessions() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => json<{ ended: number }>(await del('/auth/sessions')),
    onSuccess: () => client.invalidateQueries({ queryKey: sessionsQuery.queryKey }),
  })
}
