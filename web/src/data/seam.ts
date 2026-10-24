import { useQuery } from '@tanstack/react-query'

/* Every module in this folder is a seam: one domain, one place where a loader reading bundled
   sample data becomes a request to the server. Screens only ever import from here, so swapping a
   domain over means editing its file and nothing above it. Drop `initialData` when the loader
   starts talking to the network, and the screens get the loading state they already handle. */

export const mockKey = (...key: (string | number)[]) => ['mock', ...key]

export function useMock<T>(key: (string | number)[], load: () => T): T {
  const { data } = useQuery({ queryKey: mockKey(...key), queryFn: async () => load(), initialData: load, staleTime: Infinity })
  return data as T
}
