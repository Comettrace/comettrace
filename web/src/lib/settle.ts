import { useEffect, useState } from 'react'

export function useSettled<T>(value: T, afterMs = 300): T {
  const [settled, setSettled] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), afterMs)
    return () => window.clearTimeout(timer)
  }, [value, afterMs])
  return settled
}
