import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { roleRank, type Role } from '@/auth/api'
import { del, get, json, patch, post } from '@/auth/client'
import { useOrganizations } from '@/auth/session'

export type Member = { account: string; email: string; role: Role; joined_at_ms: number }

/* The token is returned once, by the one request that created the account, and the control plane
   keeps only its digest. It is absent when the address already had an account: choosing a
   password for one somebody else signs in with would hand that account over. */
export type Welcomed = { member: Member; invite_token: string | null }

const key = (org: string | undefined) => ['org', org, 'members']

export function useMembers() {
  const { active } = useOrganizations()
  const { data, isPending } = useQuery({
    queryKey: key(active?.org),
    enabled: Boolean(active),
    staleTime: 30_000,
    queryFn: async () => (await json<{ members: Member[] }>(await get(`/api/orgs/${active?.org}/members`))).members,
  })
  return { members: data, loading: Boolean(active) && isPending }
}

/* Every change answers with the member as the server now holds it, so the table is written from
   that rather than asked for again. */
export function useGrantRole() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (asked: { account: string; role: Role }) =>
      json<Member>(await patch(`/api/orgs/${active?.org}/members/${asked.account}`, { role: asked.role })),
    onSuccess: (changed) =>
      client.setQueryData<Member[]>(key(active?.org), (roll) =>
        roll ? sorted(roll.map((one) => (one.account === changed.account ? changed : one))) : undefined,
      ),
  })
}

export function useRemoveMember() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (account: string) => {
      await del(`/api/orgs/${active?.org}/members/${account}`)
      return account
    },
    onSuccess: (account) =>
      client.setQueryData<Member[]>(key(active?.org), (roll) => roll?.filter((one) => one.account !== account)),
  })
}

export function useInvite() {
  const { active } = useOrganizations()
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (asked: { email: string; role: Role }) =>
      json<Welcomed>(await post(`/api/orgs/${active?.org}/members`, asked)),
    onSuccess: (welcomed) =>
      client.setQueryData<Member[]>(key(active?.org), (roll) => (roll ? sorted([...roll, welcomed.member]) : undefined)),
  })
}

/* The order the server answers in, said again here so a role changed in place lands where the
   next read would have put it rather than where the row happened to be. */
export const sorted = (roll: Member[]) =>
  [...roll].sort((a, b) => roleRank[a.role] - roleRank[b.role] || a.email.localeCompare(b.email))

/* The rules the server enforces, said once here so a control can be greyed out rather than
   pressed and refused. Your own row is a row like any other: what holds the last owner in place
   is that the organisation would be left with none, not that the row is yours. */
export const mayChange = (mine: Role | undefined, theirs: Role) =>
  mine === 'owner' || (mine === 'admin' && theirs !== 'owner')

export const theLastOwner = (roll: Member[], member: Member) =>
  member.role === 'owner' && roll.filter((one) => one.role === 'owner').length <= 1

export const mayHandOut = (mine: Role | undefined): Role[] =>
  mine === 'owner' ? ['owner', 'admin', 'member'] : ['admin', 'member']

/* An address is all the server holds, so initials come from the local part: the first letter of
   each word it is spelt in, which is what most addresses are built from. */
export function initialsOf(email: string) {
  const letters = email
    .split('@')[0]
    .split(/[.\-_+]/)
    .filter(Boolean)
    .map((word) => word[0])
    .join('')
  return (letters || email.slice(0, 2)).slice(0, 2).toUpperCase()
}
