import { makeSeries } from './series'

export type IssueStatus = 'unresolved' | 'resolved' | 'ignored'
export type Issue = {
  id: string; short: string; type: string; message: string; culprit: string; service: string; platform: 'backend' | 'frontend'
  level: 'error' | 'fatal' | 'warn'; status: IssueStatus; isNew?: boolean; regressed?: boolean
  events: number; users: number; first: string; last: string; release: string; assignee: string | null; spark: number[]; team: string
}

const raw: Omit<Issue, 'spark'>[] = [
  { id: 'iss_142', short: 'STOREFRONT-142', type: 'PspTimeoutError', message: 'upstream psp-gateway timed out after 3000 ms', culprit: 'psp/client.ts in charge', service: 'checkout-api', platform: 'backend', level: 'error', status: 'unresolved', isNew: true, events: 412, users: 61, first: '27 min ago', last: '2 s ago', release: '2.14.1', assignee: 'Inès Martin', team: 'Backend' },
  { id: 'iss_141', short: 'STOREFRONT-141', type: 'TypeError', message: "Cannot read properties of undefined (reading 'token')", culprit: 'checkout.tsx in submitPayment', service: 'storefront-web', platform: 'frontend', level: 'error', status: 'unresolved', isNew: true, events: 214, users: 38, first: '26 min ago', last: '14 s ago', release: '2.14.1', assignee: null, team: 'Mobile' },
  { id: 'iss_138', short: 'PAYMENTS-38', type: 'HttpError 502', message: 'POST payments:8080/authorize returned 502 Bad Gateway', culprit: 'payments/service.ts in capture', service: 'payments', platform: 'backend', level: 'error', status: 'unresolved', events: 96, users: 27, first: '26 min ago', last: '44 s ago', release: '1.9.0', assignee: 'Marc Dubois', team: 'Payments' },
  { id: 'iss_120', short: 'DATA-20', type: 'SMTPPoolExhausted', message: 'smtp pool exhausted, worker exiting with code 1', culprit: 'mailer/pool.py in acquire', service: 'notifier', platform: 'backend', level: 'fatal', status: 'unresolved', regressed: true, events: 18, users: 0, first: 'Sep 2', last: '28 s ago', release: '0.8.3', assignee: null, team: 'Data' },
  { id: 'iss_117', short: 'STOREFRONT-117', type: 'ChunkLoadError', message: 'Loading chunk 412 failed', culprit: 'router.ts in lazyRoute', service: 'storefront-web', platform: 'frontend', level: 'error', status: 'unresolved', events: 22, users: 19, first: 'Oct 18', last: '6 min ago', release: '2.14.0', assignee: 'Sofia Rossi', team: 'Mobile' },
  { id: 'iss_109', short: 'STOREFRONT-109', type: 'ReservationConflict', message: 'reservation conflict on sku 8841-B', culprit: 'inventory/reserve.go in Reserve', service: 'inventory', platform: 'backend', level: 'warn', status: 'unresolved', events: 340, users: 112, first: 'Oct 9', last: '3 min ago', release: '3.2.0', assignee: null, team: 'Backend' },
  { id: 'iss_102', short: 'STOREFRONT-102', type: 'SecurityError', message: 'The operation is insecure', culprit: 'storage.ts in readCart', service: 'storefront-web', platform: 'frontend', level: 'error', status: 'unresolved', events: 19, users: 7, first: 'Oct 2', last: '1 h ago', release: '2.13.4', assignee: null, team: 'Mobile' },
  { id: 'iss_097', short: 'STOREFRONT-97', type: 'ResizeObserver loop', message: 'ResizeObserver loop completed with undelivered notifications', culprit: 'vendor', service: 'storefront-web', platform: 'frontend', level: 'warn', status: 'ignored', events: 6120, users: 2400, first: 'Sep 12', last: '12 s ago', release: '2.10.0', assignee: null, team: 'Mobile' },
  { id: 'iss_088', short: 'STOREFRONT-88', type: 'ValidationError', message: 'cart is empty', culprit: 'cart/validate.ts in assertNotEmpty', service: 'checkout-api', platform: 'backend', level: 'warn', status: 'resolved', events: 18, users: 14, first: 'Aug 30', last: '3 h ago', release: '2.12.0', assignee: 'Inès Martin', team: 'Backend' },
  { id: 'iss_071', short: 'PAYMENTS-21', type: 'DeadlineExceeded', message: 'context deadline exceeded calling fx-rates', culprit: 'fx/client.go in Rates', service: 'payments', platform: 'backend', level: 'error', status: 'resolved', events: 902, users: 0, first: 'Aug 12', last: 'Sep 17', release: '1.8.2', assignee: 'Marc Dubois', team: 'Payments' },
]

export const ISSUES: Issue[] = raw.map((r, k) => ({
  ...r,
  spark: makeSeries(24, 50 + k, (i, rnd) => {
    if (r.isNew) return i < 15 ? 0 : 0.3 + rnd() * 0.7
    if (r.status === 'resolved') return i < 8 ? 0.2 + rnd() * 0.3 : 0
    return 0.15 + rnd() * 0.5
  }),
}))

export const STACK = [
  { file: 'psp/client.ts', fn: 'charge', line: 88, inApp: true, code: ['  const res = await fetch(`${PSP_URL}/charge`, {', "    method: 'POST', body, signal: AbortSignal.timeout(3000),", '  })', '  if (!res.ok) throw new PspTimeoutError(`upstream psp-gateway timed out after ${ms} ms`)', '  return res.json()'], hl: 3 },
  { file: 'payments/service.ts', fn: 'Payments.capture', line: 142, inApp: true, code: ['  const idem = await cache.setIdempotency(order.id)', '  const charge = await psp.charge({ amount, currency, idem })', '  await events.emit("payment.captured", charge)'], hl: 1 },
  { file: 'checkout/create-order.ts', fn: 'createOrder', line: 61, inApp: true, code: ['  await inventory.reserve(cart.items)', '  const payment = await payments.capture(order)', '  return { order, payment }'], hl: 1 },
  { file: 'node_modules/express/lib/router/layer.js', fn: 'Layer.handle', line: 95, inApp: false, code: [], hl: 0 },
  { file: 'node:internal/process/task_queues', fn: 'processTicksAndRejections', line: 105, inApp: false, code: [], hl: 0 },
]

export const TAGS: { key: string; values: [string, number][] }[] = [
  { key: 'release', values: [['2.14.1', 100]] },
  { key: 'environment', values: [['production', 100]] },
  { key: 'host.name', values: [['app-3', 58], ['app-1', 24], ['app-2', 18]] },
  { key: 'http.route', values: [['/api/checkout', 92], ['/api/payments/refund', 8]] },
  { key: 'psp.region', values: [['eu-west', 81], ['eu-central', 19]] },
]

export const EVENTS = [
  ['14:32:01.002', '4bf92f3577b34da6a3ce929d0e0e4736', 'app-3', 'usr_31d…'],
  ['14:31:55.090', '45c48cce2e2d7fbdea1afc51c7c6ad26', 'app-1', 'usr_0a9…'],
  ['14:31:40.771', '4bf92f3577b34da6a3ce929d0e0e4736', 'app-3', 'usr_c42…'],
  ['14:31:22.310', '45c48cce2e2d7fbdea1afc51c7c6ad26', 'app-2', 'usr_9e1…'],
  ['14:30:58.004', '4bf92f3577b34da6a3ce929d0e0e4736', 'app-3', 'usr_7b7…'],
  ['14:30:31.660', '45c48cce2e2d7fbdea1afc51c7c6ad26', 'app-1', 'usr_31d…'],
]

export const ACTIVITY = [
  { who: 'Comettrace', what: 'first saw this issue in release 2.14.1', when: '14:05' },
  { who: 'Comettrace', what: 'alerted Backend in Slack #alerts-backend (rule: New exception type)', when: '14:05' },
  { who: 'Inès Martin', what: 'assigned the issue to herself', when: '14:09' },
  { who: 'Inès Martin', what: 'commented: "PSP is timing out on their side, retry with backoff is in place. Adding a second provider."', when: '14:21' },
]
