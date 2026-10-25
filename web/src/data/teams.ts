import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { del, get, json, patch, post, put } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { nothingYet } from './orgs'

export type Team = {
  id: string
  name: string
  created_at_ms: number
  members: string[]
  projects: { id: string; name: string }[]
}

const key = (org: string | undefined) => ['org', org, 'teams']

export function useTeams() {
  const found = useOrganizations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 30_000,
    queryFn: async () => (await json<{ teams: Team[] }>(await get(`/api/orgs/${active?.org}/teams`))).teams,
  })
  return { teams: data, nothing: nothingYet(found, isPending, error, 'teams') }
}

/* Every change answers with the teams as they now stand rather than with the one that moved,
   because one team is never the whole of it: handing a project to a team takes it off the team
   that held it, and a screen written from one row would be wrong about the other. */
function useArrange<T>(send: (org: string, asked: T) => Promise<{ teams: Team[] }>) {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (asked: T) => (await send(active?.org ?? '', asked)).teams,
    onSuccess: (teams) => client.setQueryData(key(active?.org), teams),
  })
}

export const useCreateTeam = () =>
  useArrange<string>(async (org, name) => json(await post(`/api/orgs/${org}/teams`, { name })))

export const useRenameTeam = () =>
  useArrange<{ team: string; name: string }>(async (org, asked) =>
    json(await patch(`/api/orgs/${org}/teams/${asked.team}`, { name: asked.name })),
  )

export const useDisbandTeam = () =>
  useArrange<string>(async (org, team) => json(await del(`/api/orgs/${org}/teams/${team}`)))

export const useSeatTeam = () =>
  useArrange<{ team: string; members: string[] }>(async (org, asked) =>
    json(await put(`/api/orgs/${org}/teams/${asked.team}/members`, { members: asked.members })),
  )

export const useOwnTeam = () =>
  useArrange<{ team: string; projects: string[] }>(async (org, asked) =>
    json(await put(`/api/orgs/${org}/teams/${asked.team}/projects`, { projects: asked.projects })),
  )

export const TEAM_NAME_MAX_BYTES = 64

/* The server owns the rule and answers 422 with it in plain text; this is the same rule checked
   early so the field can answer without a round trip. */
export function teamNameProblem(name: string): string | null {
  const kept = name.trim()
  if (new TextEncoder().encode(kept).length > TEAM_NAME_MAX_BYTES) {
    return `a team name can be at most ${TEAM_NAME_MAX_BYTES} bytes`
  }
  if (kept.length === 0) return 'a team needs a name'
  return null
}

/* Two letters drawn from the name, which is what the badge in the table holds: the initials of a
   name spelt in words, and the first two letters of one that is not. */
export function initialsOf(name: string) {
  const words = name.split(/[\s.\-_]+/).filter(Boolean)
  const letters = words.length > 1 ? words.map((word) => word[0]).join('') : name
  return letters.slice(0, 2).toUpperCase()
}
