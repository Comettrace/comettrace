import { useEffect, useState } from 'react'

/* Holds a value still until typing stops. A filter that goes to the server needs this: without
   it, "psp.charge" is eleven queries and the answer to the first keystroke arrives last. */
export function useSettled<T>(value: T, afterMs = 300): T {
  const [settled, setSettled] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), afterMs)
    return () => window.clearTimeout(timer)
  }, [value, afterMs])
  return settled
}
