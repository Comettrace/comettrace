import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { accountQuery } from '@/auth/api'
import { because, get, json, patch } from '@/auth/client'
import { useOrganizations } from '@/auth/session'

export type OrgCard = { id: string; name: string; plan: string; created_at_ms: number; version: string }

const key = (org: string | undefined) => ['org', org]

/* Three different nothings used to read as one sentence on the screen: still asking, a session
   that belongs to no organisation at all — the query is disabled then, so it neither loads nor
   fails — and a request that came back refused. Only the last is the server's to explain, and it
   explains it in plain text, so that is what gets shown. */
export function nothingYet(
  found: { active: unknown; asking: boolean },
  isPending: boolean,
  error: unknown,
  what: string,
) {
  if (found.asking) return 'Asking the server who you are…'
  if (!found.active) {
    return 'This session belongs to no organization. An operator grants one with `comettrace grant-role`.'
  }
  if (isPending) return `Reading the ${what}…`
  return because(error, `The ${what} could not be read.`)
}

export function useOrgCard() {
  const found = useOrganizations()
  const { active } = found
  const { data, isPending, error } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 60_000,
    queryFn: async () => json<OrgCard>(await get(`/api/orgs/${active?.org}`)),
  })
  return { card: data, nothing: nothingYet(found, isPending, error, 'organization') }
}

/* The rename answers with the card the server kept, so that replaces this cache rather than being
   asked for again. /auth/me carries the name too and the top bar reads it from there, so that one
   is thrown away instead. */
export function useRenameOrg() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (name: string) => json<OrgCard>(await patch(`/api/orgs/${active?.org}`, { name })),
    onSuccess: (card) => {
      client.setQueryData(key(card.id), card)
      void client.invalidateQueries({ queryKey: accountQuery.queryKey })
    },
  })
}

export const ORG_NAME_MAX_BYTES = 128

/* The server owns the rule and answers 422 with it in plain text; this is the same rule checked
   early so the field can answer without a round trip. */
export function orgNameProblem(name: string): string | null {
  const kept = name.trim()
  if (new TextEncoder().encode(kept).length > ORG_NAME_MAX_BYTES) return `a name can be at most ${ORG_NAME_MAX_BYTES} bytes`
  if (kept.length === 0) return 'an organisation needs a name'
  return null
}
