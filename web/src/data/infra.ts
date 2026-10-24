import { CLUSTERS, CONTAINERS, DOCKER_HOSTS, K8S_EVENTS, NAMESPACES, NODES, PODS, WORKLOADS } from '@/mocks/infra'
import { makeSeries } from '@/mocks/series'
import { useMock } from './seam'

export type { CStatus, PodStatus } from '@/mocks/infra'

export function useKubernetes() {
  return useMock(['infra', 'kubernetes'], () => ({
    clusters: CLUSTERS,
    namespaces: NAMESPACES,
    workloads: WORKLOADS,
    pods: PODS,
    nodes: NODES,
    events: K8S_EVENTS,
    cpu: makeSeries(60, 140, (i, r) => 52 + Math.sin(i / 8) * 5 + (i > 32 ? 6 : 0) + r() * 3),
    requested: makeSeries(60, 141, () => 64),
    memory: makeSeries(60, 142, (i, r) => 66 + i * 0.08 + r() * 2),
  }))
}

export function useDocker() {
  return useMock(['infra', 'docker'], () => ({
    hosts: DOCKER_HOSTS,
    containers: CONTAINERS,
    cpu: makeSeries(60, 150, (i, r) => 34 + Math.sin(i / 6) * 6 + (i > 40 ? 12 : 0) + r() * 4),
    pressure: makeSeries(60, 151, (i, r) => (i > 40 ? 85 + r() * 10 : 30 + r() * 15)),
  }))
}
