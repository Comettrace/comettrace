import { MEMBERS } from '@/mocks/mock'
import { useMock } from './seam'

export function useAssignees() {
  return useMock(['organisation', 'assignees'], () => MEMBERS)
}
