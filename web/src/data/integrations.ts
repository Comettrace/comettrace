import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { del, get, json, post } from '@/auth/client'
import { useOrganizations } from '@/auth/session'

/* What the deployment is wired up to, and never with what. A token is set in the deployment's
   configuration and read out of it there; nothing on this side of the wire has ever seen one. */
export type Wired = {
  linear: boolean
  github: { repository: string } | null
  jira: { base_url: string; project_key: string } | null
  mail: boolean
}

/* What this organisation has connected. Also no secret: the store does not even select the
   sealed columns to build it. */
export type Connected = {
  provider: 'linear' | 'github' | 'jira'
  account_label: string
  connected_at_ms: number
  connected_by: string | null
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

const key = (org: string | undefined) => ['org', org, 'integrations']

export function useConnections() {
  const { active } = useOrganizations()
  const { data } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 30_000,
    queryFn: async () => (await json<{ connections: Connected[] }>(await get(`/api/orgs/${active?.org}/integrations`))).connections,
  })
  return data ?? []
}

/* The server answers with the url rather than redirecting to it, because a fetch that followed a
   redirect across an origin would be stopped by the browser and would have taken nobody
   anywhere. So the browser is sent by this, not by the response. */
export function useConnect() {
  const { active } = useOrganizations()
  return useMutation({
    mutationFn: async (provider: string) => {
      const { url } = await json<{ url: string }>(await post(`/api/orgs/${active?.org}/integrations/${provider}/start`))
      window.location.assign(url)
    },
  })
}

export function useDisconnect() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (provider: string) => {
      await del(`/api/orgs/${active?.org}/integrations/${provider}`)
    },
    onSuccess: () => void client.invalidateQueries({ queryKey: key(active?.org) }),
  })
}

/* The callback sends the browser back with one word, never with the error text: a query string
   ends up in a history file, and these words name the deployment's own state. The sentences live
   here, where they can be read in the language of the screen. */
export const WHY: Record<string, string> = {
  declined: 'Consent was not given, so nothing was connected.',
  expired: 'That connect attempt had expired. Press Connect again.',
  forbidden: 'Whoever started that no longer administers this organization.',
  unkeyed: 'This deployment holds no key to seal a connection with. An operator sets COMETTRACE_SECRET_KEY.',
  unopenable: 'The deployment key changed, so the old connection cannot be read. Connect again.',
  unconfigured: 'This deployment has registered no application with that provider.',
  refused: 'The provider refused the handshake. If you reloaded the page, press Connect again.',
  unreachable: 'The provider could not be reached.',
  incomplete: 'The provider came back without a code.',
  unknown: 'That is not a provider this system knows.',
  unavailable: 'Something was out of reach. Try again.',
}
