import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { del, get, json, post, put } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { nothingYet } from '@/data/orgs'

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


/* Somewhere an issue can be filed: a Linear team, and later a GitHub repository or a Jira
   project. The id is the provider's; the name is what the screen says. */
export type Place = { id: string; name: string }

/* Where issues go for one provider. The default and the projects that differ are kept apart on
   purpose: a resolved value alone could not say which projects are following the organisation,
   and that is the sentence the screen has to be able to write. */
export type Routing = {
  provider: 'linear' | 'github' | 'jira'
  default: Place | null
  projects: { project: string; destination: Place }[]
}

const filings = (org: string | undefined) => ['org', org, 'filings']

export function useFilings() {
  const found = useOrganizations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: filings(active?.org),
    enabled: Boolean(active),
    staleTime: 30_000,
    queryFn: async () => (await json<{ routings: Routing[] }>(await get(`/api/orgs/${active?.org}/filings`))).routings,
  })
  return { routings: data, nothing: nothingYet(found, isPending, error, 'issue destinations') }
}

/* Asked of the provider every time, because teams are made and archived between visits. Only
   an administrator may ask: the answer is fetched with the organisation's credential, and
   spending it is theirs to do. */
export function useDestinations(provider: string, when: boolean) {
  const { active } = useOrganizations()
  const { data, isPending, error } = useQuery({
    queryKey: ['org', active?.org, 'filings', provider, 'destinations'],
    enabled: Boolean(active) && when,
    staleTime: 60_000,
    retry: false,
    queryFn: async () =>
      (await json<{ destinations: Place[] }>(await get(`/api/orgs/${active?.org}/filings/${provider}/destinations`))).destinations,
  })
  return { places: data, loading: when && isPending, unreachable: error }
}

/* One mutation for both altitudes, because they are the same act: no project is the
   organisation's default, and no destination clears whichever was named. The server answers with
   the whole arrangement, so the cache is replaced rather than invalidated — a screen cannot show
   a default and an override that never existed together. */
export function useAim() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ provider, project, destination }: { provider: string; project?: string; destination: string | null }) =>
      (
        await json<{ routings: Routing[] }>(
          await put(`/api/orgs/${active?.org}/filings/${provider}`, { project: project ?? null, destination }),
        )
      ).routings,
    onSuccess: (routings) => client.setQueryData(filings(active?.org), routings),
  })
}

/* Where one project's issues actually go, which is the question every screen asks and no single
   row answers: the project's own choice if it made one, the organisation's otherwise. */
export function aimedAt(routing: Routing | undefined, project: string | undefined) {
  const own = project ? routing?.projects.find((one) => one.project === project) : undefined
  return { place: own?.destination ?? routing?.default ?? null, following: !own }
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
  gone: 'That is no longer somewhere issues can be filed. Pick another.',
  incomplete: 'The provider came back without a code.',
  unknown: 'That is not a provider this system knows.',
  unavailable: 'Something was out of reach. Try again.',
}
