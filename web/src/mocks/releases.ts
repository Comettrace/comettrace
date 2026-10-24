export type Release = {
  version: string; project: string; services: string[]; deployed: string; by: string; commit: string; status: 'regressed' | 'healthy' | 'rolling'
  newIssues: number; errDelta: string; p95Delta: string; adoption: number; commits: number; prs: [string, string][]; markerAt?: number
}
export const RELEASES: Release[] = [
  { version: '2.14.1', project: 'storefront', services: ['checkout-api', 'storefront-web'], deployed: 'Today 14:04', by: 'Karim Benali', commit: 'a91f3c2', status: 'regressed', newIssues: 2, errDelta: '+3.8 pts', p95Delta: '×3.1', adoption: 100, commits: 7, markerAt: 32,
    prs: [['#1281', 'Move PSP calls to the new client with a 3 s timeout'], ['#1279', 'Checkout: read payment token from session'], ['#1276', 'Bump opentelemetry-js to 1.27.0']] },
  { version: '1.9.0', project: 'payments-eu', services: ['payments'], deployed: 'Today 11:20', by: 'Marc Dubois', commit: '4be09d1', status: 'healthy', newIssues: 0, errDelta: '+0.1 pts', p95Delta: '−8 %', adoption: 100, commits: 12, prs: [['#412', 'Refunds: idempotency keys'], ['#409', 'Retry PSP with backoff']] },
  { version: '2.14.0', project: 'storefront', services: ['storefront-web'], deployed: 'Oct 22, 16:02', by: 'Sofia Rossi', commit: '77c1ab0', status: 'healthy', newIssues: 1, errDelta: '0.0 pts', p95Delta: '−3 %', adoption: 100, commits: 19, prs: [['#1270', 'Lazy-load product routes']] },
  { version: '0.8.3', project: 'data-pipeline', services: ['notifier'], deployed: 'Oct 21, 09:15', by: 'Li Wei', commit: 'e0d4f92', status: 'healthy', newIssues: 0, errDelta: '−0.2 pts', p95Delta: '−12 %', adoption: 100, commits: 4, prs: [['#88', 'SMTP pool size from env']] },
  { version: '3.2.0', project: 'storefront', services: ['inventory'], deployed: 'Oct 9, 10:41', by: 'Inès Martin', commit: '1c2d3e4', status: 'healthy', newIssues: 1, errDelta: '+0.1 pts', p95Delta: '−20 %', adoption: 100, commits: 31, prs: [['#1201', 'Batch stock reservations']] },
]
