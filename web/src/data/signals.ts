import { BACKEND, FRONTEND, resetSeed, type SignalView } from '@/mocks/signals'
import { useMock } from './seam'

export type { SignalView }

export type SignalGroup = 'backend' | 'frontend'

export function useSignalView(group: SignalGroup, view: string) {
  return useMock(['signals', group, view], () => {
    const found = (group === 'backend' ? BACKEND : FRONTEND)[view]
    if (!found) return null
    resetSeed(view.length * 7)
    return { view: found, a: Array.from({ length: 60 }, (_, i) => found.a(i)), b: Array.from({ length: 60 }, (_, i) => found.b(i)) }
  })
}
