import { queryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { FactorKind } from '@/auth/api'
import { del, get, json, post } from '@/auth/client'

export type Standing = { live: FactorKind[]; recovery_left: number }
export type Enrolment = { kind: FactorKind; key: string; uri: string }
export type Minted = { recovery: string[] }

export const KIND_LABEL: Record<FactorKind, string> = { app: 'Authenticator app' }

export const secondFactorQuery = queryOptions({
  queryKey: ['auth', '2fa'],
  queryFn: async () => json<Standing>(await get('/auth/2fa')),
  retry: false,
  staleTime: 30_000,
})

export function useSecondFactor(offered: boolean) {
  const { data, isPending, error } = useQuery({ ...secondFactorQuery, enabled: offered })
  return { standing: data, asking: isPending, error }
}

export function useEnrol() {
  return useMutation({
    mutationFn: async () => json<Enrolment>(await post('/auth/2fa/app')),
  })
}

export function useConfirmFactor() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (code: string) => {
      const answered = await post('/auth/2fa/app/confirm', { code })
      return answered.status === 204 ? { recovery: [] } : await json<Minted>(answered)
    },
    onSuccess: () => client.invalidateQueries({ queryKey: secondFactorQuery.queryKey }),
  })
}

export function useForgetFactor() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (kind: FactorKind) => {
      await del(`/auth/2fa/${kind}`)
    },
    onSuccess: () => client.invalidateQueries({ queryKey: secondFactorQuery.queryKey }),
  })
}

export function useRegenerateRecovery() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async () => json<Minted>(await post('/auth/2fa/recovery')),
    onSuccess: () => client.invalidateQueries({ queryKey: secondFactorQuery.queryKey }),
  })
}
