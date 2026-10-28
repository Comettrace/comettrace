export type Sev = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal'

const SVC: Record<string, number> = { 'api-gateway': 1, 'checkout-api': 2, checkout: 2, inventory: 3, payments: 4, auth: 5, notifier: 6, 'webhook-sender': 7, 'unknown_service:node': 8, postgresql: 8, redis: 8 }

export const svcIndex = (name: string) => {
  if (SVC[name]) return SVC[name]
  let h = 0
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return (h % 8) + 1
}
