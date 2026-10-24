import { SERVICES, type Service } from '@/mocks/mock'
import { useMock } from './seam'

export type { Service }

export function useServices(): Service[] {
  return useMock(['services'], () => SERVICES)
}
