import { ALERT_RULES, INGEST_KEYS, ME, MEMBERS, PROJECTS, QUERY_KEYS } from '@/mocks/mock'
import { useMock } from './seam'

/* What is left of the sample data. The organisation itself is real (see src/data/orgs), so are
   its members (src/data/members) and its teams (src/data/teams), and so are memberships and
   projects (src/auth, src/data/projects); no key endpoint exists yet.

   The two lists below yield the names they used to hold, because a real list now answers to them.
   The assignees are the cast the issue screens pick from: issues are sample data through and
   through, so the people on them are too. The sampled projects are the ones the overview invents
   an ingest rate for, positionally — three projects, three rates — so pointing that card at the
   real catalogue would put a made-up rate against a real name, which is worse than this. */

export function useSampledProjects() {
  return useMock(['organization', 'projects'], () => PROJECTS)
}

export function useAssignees() {
  return useMock(['organization', 'assignees'], () => MEMBERS)
}

export function useIngestKeys() {
  return useMock(['organization', 'ingest-keys'], () => INGEST_KEYS)
}

export function useReadKeys() {
  return useMock(['organization', 'read-keys'], () => QUERY_KEYS)
}

export function useExceptionAlerts() {
  return useMock(['organization', 'exception-alerts'], () => ALERT_RULES)
}

export function useProfile() {
  return useMock(['organization', 'profile'], () => ME)
}
