import { ALERT_RULES, INGEST_KEYS, ME, MEMBERS, PROJECTS, QUERY_KEYS, TEAMS } from '@/mocks/mock'
import { useMock } from './seam'

/* What is left of the sample data. The organisation itself is real (see src/data/orgs), and so
   are memberships and projects (src/auth, src/data/projects); no member, team or key endpoint
   exists yet, and no table holds a team at all. */

export function useProjects() {
  return useMock(['organization', 'projects'], () => PROJECTS)
}

export function useMembers() {
  return useMock(['organization', 'members'], () => MEMBERS)
}

export function useTeams() {
  return useMock(['organization', 'teams'], () => TEAMS)
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
