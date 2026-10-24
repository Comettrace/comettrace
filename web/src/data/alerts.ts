import { HISTORY, RULES, type Rule, type Severity } from '@/mocks/alerts'
import { makeSeries } from '@/mocks/series'
import { useMock } from './seam'

export type { Rule, Severity }

export function useAlertRules(): Rule[] {
  return useMock(['alerts', 'rules'], () => RULES)
}

export function useAlertHistory() {
  return useMock(['alerts', 'history'], () => HISTORY)
}

export function useFiringSeries(rule: Rule, seed: number): number[] {
  return useMock(['alerts', rule.id, 'series'], () => makeSeries(60, 80 + seed, (i, r) => (rule.id === 'r3' ? 80 + i * 0.14 + r() * 1.5 : i >= 33 ? 70 + r() * 25 : 20 + r() * 15)))
}

export function usePreviewSeries(): number[] {
  return useMock(['alerts', 'preview'], () => makeSeries(60, 99, (_i, r) => 500 + r() * 500))
}
