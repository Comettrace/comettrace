export function seeded(seed: number) {
  let s = seed
  return () => {
    s = (s * 9301 + 49297) % 233280
    return s / 233280
  }
}

export function makeSeries(n: number, seed: number, f: (i: number, r: () => number) => number) {
  const r = seeded(seed)
  return Array.from({ length: n }, (_, i) => f(i, r))
}
