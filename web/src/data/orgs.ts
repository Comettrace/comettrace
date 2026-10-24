import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { accountQuery } from '@/auth/api'
import { get, json, patch } from '@/auth/client'
import { useOrganizations } from '@/auth/session'

export type OrgCard = { id: string; name: string; plan: string; created_at_ms: number; version: string }

const key = (org: string | undefined) => ['org', org]

export function useOrgCard() {
  const { active } = useOrganizations()
  const { data, isPending } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 60_000,
    queryFn: async () => json<OrgCard>(await get(`/api/orgs/${active?.org}`)),
  })
  return { card: data, loading: Boolean(active) && isPending }
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
