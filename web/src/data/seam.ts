import { useQuery } from '@tanstack/react-query'

export const mockKey = (...key: (string | number)[]) => ['mock', ...key]

export function useMock<T>(key: (string | number)[], load: () => T): T {
  const { data } = useQuery({ queryKey: mockKey(...key), queryFn: async () => load(), initialData: load, staleTime: Infinity })
  return data as T
}
