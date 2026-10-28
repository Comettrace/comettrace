import { queryOptions } from '@tanstack/react-query'
import { get, json, patch, post } from './client'

export type Role = 'owner' | 'admin' | 'member'
export type Membership = { org: string; name: string; role: Role }
export type Account = { account: string; email: string; name: string | null; photo: string | null; active_org: string | null; memberships: Membership[] }
export type Methods = { local_password: boolean; oidc: boolean }

export const accountQuery = queryOptions({
  queryKey: ['auth', 'me'],
  queryFn: async () => json<Account>(await get('/auth/me')),
  retry: false,
  staleTime: 30_000,
})

export const methodsQuery = queryOptions({
  queryKey: ['auth', 'methods'],
  queryFn: async () => {
    try {
      return await json<Methods>(await get('/auth/methods'))
    } catch {
      return { local_password: true, oidc: false }
    }
  },
  retry: false,
  staleTime: Infinity,
})

export const healthQuery = queryOptions({
  queryKey: ['health'],
  queryFn: async () => (await (await get('/health')).text()).trim(),
  retry: false,
  refetchInterval: 30_000,
})

export type FactorKind = 'app'
export type Admitted = { kind: 'session' } | { kind: 'second-step'; offered: FactorKind[] }

export async function signIn(email: string, password: string): Promise<Admitted> {
  const response = await post('/auth/sign-in', { email, password })
  if (response.status === 204) return { kind: 'session' }
  const asked = await json<{ second_step: FactorKind[] }>(response)
  return { kind: 'second-step', offered: asked.second_step }
}

export const verifySecondStep = (answer: { code: string } | { recovery: string }) => post('/auth/2fa/verify', answer)
export const signOut = () => post('/auth/sign-out')
export const requestReset = (email: string) => post('/auth/reset/request', { email })
export const confirmReset = (token: string, password: string) => post('/auth/reset/confirm', { token, password })

export const renameMe = (name: string) => patch('/auth/me', { name })

export const OIDC_START = '/auth/oidc/start'

export const PASSWORD_MIN = 12
const PASSWORD_MAX_BYTES = 128

export function passwordProblem(password: string): string | null {
  if (password.trim().length === 0) return 'a password cannot be only whitespace'
  if (password.length < PASSWORD_MIN) return `a password needs at least ${PASSWORD_MIN} characters`
  if (new TextEncoder().encode(password).length > PASSWORD_MAX_BYTES) return `a password can be at most ${PASSWORD_MAX_BYTES} bytes`
  return null
}

export const roleRank: Record<Role, number> = { owner: 0, admin: 1, member: 2 }
