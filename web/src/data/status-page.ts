import { INCIDENT_UPDATES, STATUS_GROUPS } from '@/mocks/mock'
import { useMock } from './seam'

export type Accent = 'indigo' | 'teal' | 'orange' | 'rose' | 'slate'
export type StatusConfig = {
  name: string; accent: Accent; days: 30 | 60 | 90; showUptime: boolean; showDesc: boolean; showPast: boolean; showSubscribe: boolean; poweredBy: boolean
  state: 'incident' | 'operational' | 'maintenance'; hidden: string[]; updates: { status: string; time: string; text: string }[]
}

const DEFAULT_STATUS: StatusConfig = {
  name: 'Acme', accent: 'teal', days: 90, showUptime: true, showDesc: true, showPast: true, showSubscribe: true, poweredBy: true,
  state: 'incident', hidden: [], updates: INCIDENT_UPDATES,
}

export function useStatusGroups() {
  return useMock(['status-page', 'groups'], () => STATUS_GROUPS)
}

export function useStatusConfig(): StatusConfig {
  return useMock(['status-page', 'config'], () => DEFAULT_STATUS)
}
