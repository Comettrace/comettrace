export const pct = (v: number, d = 1) => `${v.toFixed(d)} %`
export const num = (v: number) => v.toLocaleString('en-US')
export function dur(ms: number) {
  if (ms < 1) return `${Math.round(ms * 1000)} µs`
  if (ms < 1000) return `${ms < 10 ? ms.toFixed(1) : Math.round(ms)} ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(ms < 10000 ? 3 : 1)} s`
  const m = Math.floor(ms / 60000)
  return `${m} m ${Math.round((ms % 60000) / 1000)} s`
}
export const shortId = (id: string) => `${id.slice(0, 8)}…${id.slice(-4)}`
