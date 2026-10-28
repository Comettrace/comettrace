import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { del, get, json, patch, post, put } from '@/auth/client'
import { useOrganisations } from '@/auth/session'
import { nothingYet } from './orgs'

export type Project = {
  id: string
  name: string
  created_at_ms: number
  team: { id: string; name: string } | null
  keys: number
}

const key = (org: string | undefined) => ['projects', org]

export function useCatalogue() {
  const found = useOrganisations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 60_000,
    queryFn: async () => (await json<{ projects: Project[] }>(await get(`/api/orgs/${active?.org}/projects`))).projects,
  })
  return { projects: data, nothing: nothingYet(found, isPending, error, 'projects') }
}

export function useProjects(): Project[] {
  return useCatalogue().projects ?? []
}

function useArrange<T>(send: (org: string, asked: T) => Promise<{ projects: Project[] }>) {
  const { active } = useOrganisations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (asked: T) => (await send(active?.org ?? '', asked)).projects,
    onSuccess: (projects) => {
      client.setQueryData(key(active?.org), projects)
      void client.invalidateQueries({ queryKey: ['org', active?.org, 'teams'] })
    },
  })
}

export const useCreateProject = () =>
  useArrange<string>(async (org, name) => json(await post(`/api/orgs/${org}/projects`, { name })))

export const useRenameProject = () =>
  useArrange<{ project: string; name: string }>(async (org, asked) =>
    json(await patch(`/api/orgs/${org}/projects/${asked.project}`, { name: asked.name })),
  )

export const useForgetProject = () =>
  useArrange<string>(async (org, project) => json(await del(`/api/orgs/${org}/projects/${project}`)))

export const useAssignProject = () =>
  useArrange<{ project: string; team: string | null }>(async (org, asked) =>
    json(await put(`/api/orgs/${org}/projects/${asked.project}/team`, { team: asked.team })),
  )

export const PROJECT_NAME_MAX_BYTES = 64

export function projectNameProblem(name: string): string | null {
  const kept = name.trim().toLowerCase()
  if (new TextEncoder().encode(kept).length > PROJECT_NAME_MAX_BYTES) {
    return `a project name can be at most ${PROJECT_NAME_MAX_BYTES} bytes`
  }
  if (kept.length === 0) return 'a project needs a name'
  if (!/^[a-z0-9][a-z0-9-]*$/.test(kept)) {
    return 'a project name holds lowercase letters, digits and dashes, and starts with one'
  }
  return null
}
