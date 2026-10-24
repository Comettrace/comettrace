import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { Toaster } from 'sonner'
import { isUnauthorized } from '@/auth/client'
import { ThemeProvider, useTheme } from '@/lib/theme'
import { createAppRouter } from './routes'
import './styles/globals.css'

/* A session can end under any request, so the answer is the same wherever the 401 came from:
   go to sign-in once, carrying the path to come back to. */
function toSignIn(error: unknown) {
  if (!isUnauthorized(error)) return
  const { pathname, href } = router.state.location
  if (pathname === '/sign-in') return
  void router.navigate({ to: '/sign-in', search: { redirect: href }, replace: true })
}

const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: toSignIn }),
  mutationCache: new MutationCache({ onError: toSignIn }),
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
})

const router = createAppRouter(queryClient)

function ThemedToaster() {
  const { dark } = useTheme()
  return <Toaster theme={dark ? 'dark' : 'light'} position="bottom-right" toastOptions={{ className: 'text-control' }} />
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
        <ThemedToaster />
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
)
