import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Field, Input } from '@/components/ui/input'
import { Alert } from '@/components/ui/misc'
import { OIDC_START, accountQuery, methodsQuery, signIn } from '@/auth/api'
import { ApiError } from '@/auth/client'
import { Link, useNavigate, useSearchParam } from '@/lib/router'
import { AuthFrame } from './frame'

const refusal = (error: unknown) => {
  if (!(error instanceof ApiError)) return 'The server could not be reached.'
  if (error.status === 401) return 'That email and password do not match an account.'
  if (error.status === 501) return 'Passwords are turned off on this server. Use single sign-on.'
  if (error.status === 503) return 'The server cannot sign anyone in right now. Try again in a moment.'
  return error.body || 'The server refused the sign-in.'
}

export function SignIn() {
  const { data: methods } = useQuery(methodsQuery)
  const client = useQueryClient()
  const navigate = useNavigate()
  const back = useSearchParam('redirect')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const submit = useMutation({
    mutationFn: () => signIn(email, password),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: accountQuery.queryKey })
      navigate(back?.startsWith('/') ? back : '/', { replace: true })
    },
  })
  const locked = !methods?.local_password && !methods?.oidc
  return (
    <AuthFrame
      title="Sign in"
      intro={back?.startsWith('/') ? 'Your session ended. Sign in to pick up where you were.' : undefined}
      footer={methods?.local_password ? <>No account yet? An administrator creates one with <span className="font-mono">comettrace create-account</span>.</> : null}
    >
      {submit.error ? (
        <Alert variant="destructive" icon={<TriangleAlert />}>
          {refusal(submit.error)}
        </Alert>
      ) : null}
      {locked ? <Alert variant="warning" icon={<TriangleAlert />}>This server has neither local passwords nor an identity provider configured, so nobody can sign in yet.</Alert> : null}
      {methods?.oidc ? (
        <Button type="button" className="h-10" onClick={() => window.location.assign(OIDC_START)}>
          Continue with single sign-on
        </Button>
      ) : null}
      {methods?.oidc && methods.local_password ? (
        <div className="flex items-center gap-3 text-caption text-muted-foreground">
          <span className="h-px flex-1 bg-border" />
          or
          <span className="h-px flex-1 bg-border" />
        </div>
      ) : null}
      {methods?.local_password ? (
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault()
            submit.mutate()
          }}
        >
          <Field label="Email" htmlFor="si-email">
            <Input id="si-email" type="email" required autoFocus autoComplete="email" className="h-10" value={email} onChange={(e) => setEmail(e.target.value)} />
          </Field>
          <Field label="Password" htmlFor="si-password">
            <Input id="si-password" type="password" required autoComplete="current-password" className="h-10" value={password} onChange={(e) => setPassword(e.target.value)} />
          </Field>
          <Button type="submit" variant="primary" size="lg" loading={submit.isPending} disabled={!email || !password}>
            Sign in
          </Button>
          <Link to="/forgot-password" className="text-control">
            Forgot your password?
          </Link>
        </form>
      ) : null}
    </AuthFrame>
  )
}
