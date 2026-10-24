import { ALERT_RULES, INGEST_KEYS, ME, MEMBERS, PROJECTS, QUERY_KEYS, TEAMS, WORKSPACE } from '@/mocks/mock'
import { useMock } from './seam'

/* The server knows organisations and memberships (see src/auth), but nothing here: no project,
   member, team or key endpoint exists yet, and /auth/me carries no display name or email. */

export function useWorkspace() {
  return useMock(['organization', 'workspace'], () => WORKSPACE)
}

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
