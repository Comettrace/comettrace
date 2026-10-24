import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { accountQuery, roleRank, signOut, type Membership } from './api'
import { useNavigate } from '@/lib/router'

export function useAccount() {
  return useQuery(accountQuery).data
}

export function useOrganizations(): { list: Membership[]; active: Membership | undefined } {
  const account = useAccount()
  const list = [...(account?.memberships ?? [])].sort(
    (a, b) => roleRank[a.role] - roleRank[b.role] || a.name.localeCompare(b.name),
  )
  const active = list.find((m) => m.org === account?.active_org) ?? list[0]
  return { list, active }
}

export function useSignOut() {
  const client = useQueryClient()
  const navigate = useNavigate()
  return useMutation({
    mutationFn: signOut,
    onSettled: () => {
      client.clear()
      navigate('/sign-in', { replace: true })
    },
  })
}
