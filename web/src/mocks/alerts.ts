export type Severity = 'critical' | 'warning'
export type Rule = {
  id: string; name: string; signal: 'Spans' | 'Logs' | 'Metrics' | 'Exceptions' | 'Uptime'; condition: string; scope: string
  threshold: string; current: string; severity: Severity; notify: string; on: boolean; firing: boolean; since?: string; project: string
}
export const RULES: Rule[] = [
  { id: 'r1', name: 'checkout-api error rate', signal: 'Spans', condition: 'error rate > 2 % for 5 min', scope: 'service = checkout-api', threshold: '2 %', current: '4.1 %', severity: 'critical', notify: 'Backend · Slack #alerts-backend, PagerDuty', on: true, firing: true, since: '14:05', project: 'storefront' },
  { id: 'r2', name: 'Checkout p95 latency', signal: 'Spans', condition: 'p95 duration > 1 s for 10 min', scope: 'http.route = /api/checkout', threshold: '1 s', current: '1.9 s', severity: 'warning', notify: 'Backend · Slack #alerts-backend', on: true, firing: true, since: '14:11', project: 'storefront' },
  { id: 'r3', name: 'Disk almost full', signal: 'Metrics', condition: 'system.filesystem.utilization > 85 %', scope: 'host = worker-*', threshold: '85 %', current: '88 %', severity: 'warning', notify: 'Data · Slack #data-oncall', on: true, firing: true, since: '09:42', project: 'data-pipeline' },
  { id: 'r4', name: 'Fatal logs', signal: 'Logs', condition: 'count of severity = fatal > 0 in 1 min', scope: 'all services', threshold: '0', current: '0', severity: 'critical', notify: 'Backend · PagerDuty', on: true, firing: false, project: 'storefront' },
  { id: 'r5', name: 'New exception type', signal: 'Exceptions', condition: 'an exception.type never seen before', scope: 'project storefront', threshold: '—', current: '—', severity: 'warning', notify: 'Backend · Slack #alerts-backend', on: true, firing: false, project: 'storefront' },
  { id: 'r6', name: 'api.acme.dev is down', signal: 'Uptime', condition: 'HTTP check fails from 2 of 3 regions', scope: 'GET https://api.acme.dev/healthz every 60 s', threshold: '2 regions', current: '0', severity: 'critical', notify: 'Backend · PagerDuty, status page', on: true, firing: false, project: 'storefront' },
  { id: 'r7', name: 'Payments queue backlog', signal: 'Metrics', condition: 'messaging.queue.depth > 5,000 for 5 min', scope: 'queue = payments', threshold: '5,000', current: '1,240', severity: 'warning', notify: 'Payments · Slack #alerts-payments', on: true, firing: false, project: 'payments-eu' },
  { id: 'r8', name: 'Webhook sender silent', signal: 'Spans', condition: 'no spans for 10 min', scope: 'service = webhook-sender', threshold: '10 min', current: '23 min', severity: 'warning', notify: 'Data · Slack #data-oncall', on: false, firing: false, project: 'data-pipeline' },
]
export const HISTORY = [
  ['14:11', 'Checkout p95 latency', 'warning', 'Firing', '21 min', '—'],
  ['14:05', 'checkout-api error rate', 'critical', 'Firing', '27 min', 'Inès Martin'],
  ['09:42', 'Disk almost full', 'warning', 'Firing', '4 h 50 min', '—'],
  ['Oct 23, 22:14', 'Payments queue backlog', 'warning', 'Resolved', '12 min', 'Marc Dubois'],
  ['Oct 22, 03:02', 'api.acme.dev is down', 'critical', 'Resolved', '4 min', 'auto-resolved'],
  ['Oct 20, 16:40', 'Fatal logs', 'critical', 'Resolved', '1 min', 'Karim Benali'],
] as const
