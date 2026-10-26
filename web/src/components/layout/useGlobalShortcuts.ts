import { useEffect, useMemo, useState } from 'react'
import { ALL_NAV } from './nav'
import { useNavigate } from '@/lib/router'

export function useGlobalShortcuts() {
  const [palette, setPalette] = useState(false)
  const [shortcuts, setShortcuts] = useState(false)
  const navigate = useNavigate()
  const goKeys = useMemo(() => Object.fromEntries([...ALL_NAV.filter((i) => i.keys).map((i) => [i.keys!.toLowerCase(), i.to]), ['s', '/settings']]), [])
  useEffect(() => {
    let g = false
    let t: number | undefined
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement
      if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable)) return
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette(true); return }
      if (e.key === '/' && !e.metaKey && !e.ctrlKey) { e.preventDefault(); setPalette(true); return }
      if (e.key === '?') { setShortcuts(true); return }
      if (g) {
        const to = goKeys[e.key.toLowerCase()]
        g = false
        if (to) { e.preventDefault(); navigate(to) }
        return
      }
      if (e.key.toLowerCase() === 'g' && !e.metaKey && !e.ctrlKey) { g = true; window.clearTimeout(t); t = window.setTimeout(() => (g = false), 1200) }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [goKeys, navigate])
  return { palette, setPalette, shortcuts, setShortcuts }
}
