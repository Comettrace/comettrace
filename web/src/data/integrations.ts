import { useQuery } from '@tanstack/react-query'
import { get, json } from '@/auth/client'

/* What the deployment is wired up to, and never with what. A token is set in the deployment's
   configuration file and read out of it there; nothing on this side of the wire has ever seen
   one, which is why there is nothing here to save. */
export type Wired = {
  linear: boolean
  github: { repository: string } | null
  jira: { base_url: string; project_key: string } | null
  mail: boolean
}

export function useWired() {
  const { data, isPending, error } = useQuery({
    queryKey: ['integrations'],
    staleTime: Infinity,
    retry: false,
    queryFn: async () => json<Wired>(await get('/api/integrations')),
  })
  return { wired: data, loading: isPending, unreachable: Boolean(error) }
}
