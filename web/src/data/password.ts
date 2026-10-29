import { useMutation } from '@tanstack/react-query'
import { post } from '@/auth/client'

export const MIN_PASSWORD_CHARS = 12
export const MAX_PASSWORD_BYTES = 128

export function passwordProblem(chosen: string): string | null {
  if (new TextEncoder().encode(chosen).length > MAX_PASSWORD_BYTES) {
    return `a password cannot be longer than ${MAX_PASSWORD_BYTES} bytes`
  }
  if ([...chosen].length < MIN_PASSWORD_CHARS) {
    return `a password needs at least ${MIN_PASSWORD_CHARS} characters`
  }
  if (chosen.trim().length === 0) return 'a password cannot be only spaces'
  return null
}

export function useChangePassword() {
  return useMutation({
    mutationFn: async (chosen: { current: string; password: string }) => {
      await post('/auth/password', chosen)
    },
  })
}
