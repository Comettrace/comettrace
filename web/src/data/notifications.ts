import { useQueryClient } from '@tanstack/react-query'
import { NOTIFICATIONS, type Notif } from '@/mocks/mock'
import { mockKey, useMock } from './seam'

export type { Notif }

const LIST = ['notifications']

export function useNotifications() {
  const client = useQueryClient()
  const items = useMock(LIST, () => NOTIFICATIONS)
  const write = (f: (n: Notif) => Notif) => client.setQueryData<Notif[]>(mockKey(...LIST), (current) => (current ?? NOTIFICATIONS).map(f))
  return {
    items,
    unread: items.filter((n) => n.unread).length,
    markAll: () => write((n) => ({ ...n, unread: false })),
    mark: (id: string) => write((n) => (n.id === id ? { ...n, unread: false } : n)),
  }
}
