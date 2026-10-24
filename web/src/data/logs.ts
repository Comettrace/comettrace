import { LOGS, type LogRow } from '@/mocks/mock'
import { useMock } from './seam'

export type { LogRow }

export function useLogs(): LogRow[] {
  return useMock(['logs'], () => LOGS)
}
