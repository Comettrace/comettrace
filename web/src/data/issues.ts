import { useQueryClient } from '@tanstack/react-query'
import { ACTIVITY, EVENTS, ISSUES, STACK, TAGS, type Issue, type IssueStatus } from '@/mocks/issues'
import { makeSeries } from '@/mocks/series'
import { mockKey, useMock } from './seam'

export type { Issue, IssueStatus }

const LIST = ['issues']

export function useIssues() {
  const client = useQueryClient()
  const issues = useMock(LIST, () => ISSUES)
  const update = (ids: string[], patch: Partial<Issue>) =>
    client.setQueryData<Issue[]>(mockKey(...LIST), (current) => (current ?? ISSUES).map((i) => (ids.includes(i.id) ? { ...i, ...patch } : i)))
  return { issues, update }
}

export type Comment = { who: string; what: string; when: string }

export function useIssueDetail(issue: Issue | undefined) {
  return useMock(['issues', issue?.id ?? 'unknown', 'detail'], () => ({
    stack: STACK,
    tags: TAGS,
    events: EVENTS,
    activity: ACTIVITY as Comment[],
    series: makeSeries(60, issue?.events ?? 1, (i, r) => (issue?.isNew ? (i < 33 ? 0 : 12 + (r() - 0.4) * 8) : issue?.status === 'resolved' ? (i < 20 ? 3 + r() * 2 : 0) : 2 + r() * 4)),
  }))
}
