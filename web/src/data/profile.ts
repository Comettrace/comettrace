import { useMutation, useQueryClient } from '@tanstack/react-query'
import { accountQuery, renameMe, type Account } from '@/auth/api'
import { del, json, offer } from '@/auth/client'
import { useAccount, useOrganizations } from '@/auth/session'
import { useMembers } from './members'
import { useTeams } from './teams'

/* The name when somebody wrote one, and the address when nobody did. Every screen that shows a
   person shows this, so the fallback is said once here rather than at each of them. */
export const nameOf = (who: { email: string; name?: string | null }) => who.name ?? who.email

/* Initials for an avatar: the first letter of each word the spelling is built from. An address is
   spelt in its local part, with dots or dashes where a name has spaces, so one rule reads both. */
export function initialsOf(spelling: string) {
  const letters = spelling
    .split('@')[0]
    .split(/[.\-_+\s]/)
    .filter(Boolean)
    .map((word) => word[0])
    .join('')
  return (letters || spelling.slice(0, 2)).slice(0, 2).toUpperCase()
}

export const ACCOUNT_NAME_MAX_BYTES = 128

/* The server owns the rule and answers 422 with it in plain text; this is the same rule checked
   early so the field can answer without a round trip. An empty spelling is no problem at all: it
   is how the name is taken off again. */
export function accountNameProblem(name: string): string | null {
  if (new TextEncoder().encode(name.trim()).length > ACCOUNT_NAME_MAX_BYTES) {
    return `a name can be at most ${ACCOUNT_NAME_MAX_BYTES} bytes`
  }
  return null
}

/* The rename answers with the standing the server kept, so that replaces this cache rather than
   being asked for again. Everything that shows a name reads it from there. */
export function useRenameMe() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (name: string) => json<Account>(await renameMe(name)),
    onSuccess: (standing) => client.setQueryData(accountQuery.queryKey, standing),
  })
}

export const MAX_PHOTO_BYTES = 1024 * 1024

/* The ceiling the server enforces, checked here so a file that is plainly too big is refused
   before a megabyte of it goes up the wire. What the file *is* stays the server's to judge: it
   reads the first bytes, where a browser only reports what the extension suggested, and refusing
   a real png because a picker left its type blank would be our mistake rather than its. */
export function photoProblem(file: File): string | null {
  if (file.size > MAX_PHOTO_BYTES) return 'a photo can be at most 1 MB'
  return null
}

/* Both answer with the standing the server kept, so everything that shows a face reads the new
   one -- or the initials again -- from the same cache the top bar does. */
export function usePutPhoto() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (file: File) => json<Account>(await offer('/auth/me/photo', file)),
    onSuccess: (standing) => client.setQueryData(accountQuery.queryKey, standing),
  })
}

export function useRemovePhoto() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => json<Account>(await del('/auth/me/photo')),
    onSuccess: (standing) => client.setQueryData(accountQuery.queryKey, standing),
  })
}

/* What the active organisation holds about me, read from the two lists its screens already read:
   the roll says when I joined, the teams say which of them I sit in. A plain member may read
   both, so this is not an administrator's screen. */
export function useMyStanding() {
  const account = useAccount()
  const { active } = useOrganizations()
  const { members, nothing: noRoll } = useMembers()
  const { teams, nothing: noTeams } = useTeams()
  return {
    active,
    mine: members?.find((one) => one.account === account?.account),
    noRoll,
    teams: teams?.filter((team) => team.members.includes(account?.account ?? '')),
    noTeams,
  }
}
