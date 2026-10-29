import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { get, json, patch } from '@/auth/client'
import { useOrganisations } from '@/auth/session'
import { useScope } from '@/lib/scope'
import { nothingYet } from './orgs'

export type IssueStatus = 'unresolved' | 'resolved' | 'ignored'

export type Issue = {
  fingerprint: string
  project: string
  exception_type: string
  message: string
  service: string
  events: number
  first_seen_at_ms: number
  last_seen_at_ms: number
  status: IssueStatus
  owner: string | null
  decided_at_ms: number
  regressed: boolean
}

export type Occurrence = {
  trace_id: string
  span_id: string
  at_ms: number
  service: string
  exception_type: string
  message: string
  stacktrace: string | null
}

export const RANGES: [string, number][] = [
  ['1 hour', 60 * 60 * 1000],
  ['24 hours', 24 * 60 * 60 * 1000],
  ['7 days', 7 * 24 * 60 * 60 * 1000],
  ['30 days', 30 * 24 * 60 * 60 * 1000],
]

export const COUNTS = [50, 100, 200, 500]

export type Sift = { rangeMs: number; service: string; q: string; limit: number }

export const ANY: Sift = { rangeMs: 24 * 60 * 60 * 1000, service: '', q: '', limit: 50 }

const FRESH_MS = 15_000

export const shortPrint = (print: string) => print.slice(0, 8)

function over(rangeMs: number): URLSearchParams {
  const to = Date.now()
  return new URLSearchParams({ from: String(to - rangeMs), to: String(to) })
}

function asked(sift: Sift, project: string | undefined): string {
  const query = over(sift.rangeMs)
  if (project) query.set('project', project)
  if (sift.service) query.set('service', sift.service)
  if (sift.q.trim()) query.set('q', sift.q.trim())
  if (sift.limit !== ANY.limit) query.set('limit', String(sift.limit))
  return query.toString()
}

const LIST = 'issues'

export function useIssues(sift: Sift = ANY) {
  const found = useOrganisations()
  const { active } = found
  const { project } = useScope()
  const { data, isPending, isFetching, error } = useQuery({
    queryKey: [LIST, active?.org, project, sift],
    enabled: Boolean(active),
    staleTime: FRESH_MS,
    queryFn: async () =>
      (
        await json<{ issues: Issue[] }>(
          await get(`/api/orgs/${active?.org}/issues?${asked(sift, project)}`),
        )
      ).issues,
  })
  return {
    issues: data ?? [],
    loading: isFetching,
    nothing: nothingYet(found, isPending, error, 'issues'),
    failed: error,
  }
}

export function useIssue(print: string | undefined, rangeMs: number) {
  const found = useOrganisations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: [LIST, active?.org, 'one', print, rangeMs],
    enabled: Boolean(active && print),
    staleTime: FRESH_MS,
    queryFn: async () =>
      json<Issue>(
        await get(`/api/orgs/${active?.org}/issues/${print}?${over(rangeMs).toString()}`),
      ),
  })
  return { issue: data, nothing: nothingYet(found, isPending, error, 'issue'), failed: error }
}

export function useOccurrences(issue: Issue | undefined, rangeMs: number) {
  const { active } = useOrganisations()
  const { data, isPending, error } = useQuery({
    queryKey: [LIST, active?.org, 'occurrences', issue?.fingerprint, rangeMs],
    enabled: Boolean(active && issue),
    staleTime: FRESH_MS,
    queryFn: async () => {
      const query = over(rangeMs)
      query.set('project', issue?.project ?? '')
      return (
        await json<{ occurrences: Occurrence[] }>(
          await get(
            `/api/orgs/${active?.org}/issues/${issue?.fingerprint}/occurrences?${query.toString()}`,
          ),
        )
      ).occurrences
    },
  })
  return { occurrences: data ?? [], asking: isPending, failed: error }
}

export type Said = { issue: Issue; status?: IssueStatus; owner?: string | null }

export function useDecide() {
  const { active } = useOrganisations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (said: Said) => {
      const body: Record<string, unknown> = { project: said.issue.project }
      if (said.status) body.status = said.status
      if (said.owner !== undefined) body.owner = said.owner
      await patch(`/api/orgs/${active?.org}/issues/${said.issue.fingerprint}`, body)
    },
    onSuccess: () => client.invalidateQueries({ queryKey: [LIST, active?.org] }),
  })
}
