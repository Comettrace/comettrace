import { useQuery } from '@tanstack/react-query'
import { get, json } from '@/auth/client'
import { useOrganizations } from '@/auth/session'

export type Project = { id: string; name: string }

export function useProjects(): Project[] {
  const { active } = useOrganizations()
  const { data } = useQuery({
    queryKey: ['projects', active?.org],
    enabled: Boolean(active),
    staleTime: 60_000,
    queryFn: async () => {
      const held = await json<{ projects: Project[] }>(await get(`/api/orgs/${active?.org}/projects`))
      return held.projects
    },
  })
  return data ?? []
}
