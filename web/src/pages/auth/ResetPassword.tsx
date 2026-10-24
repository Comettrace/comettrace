import { useMutation } from '@tanstack/react-query'
import { TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Field, Input } from '@/components/ui/input'
import { Alert } from '@/components/ui/misc'
import { PASSWORD_MIN, confirmReset, passwordProblem } from '@/auth/api'
import { ApiError } from '@/auth/client'
import { Link, useNavigate, useSearchParam } from '@/lib/router'
import { AuthFrame } from './frame'

/* A 422 carries the rule that refused the password as plain text, which is better than anything
   this screen could word on its own, so it is shown as it arrives. */
const refusal = (error: unknown) => {
  if (!(error instanceof ApiError)) return 'The server could not be reached.'
  if (error.status === 422) return error.body
  if (error.status === 400) return 'This link is spent or was never issued. Ask for a new one.'
  if (error.status === 503) return 'The server cannot set a password right now. Try again in a moment.'
  return error.body || 'The server refused the new password.'
}

export function ResetPassword() {
  const token = useSearchParam('token')
  const navigate = useNavigate()
  const [password, setPassword] = useState('')
  const [again, setAgain] = useState('')
  const [problem, setProblem] = useState<string | null>(null)
  const submit = useMutation({
    mutationFn: () => confirmReset(token ?? '', password),
    onSuccess: () => {
      toast.success('Password set', { description: 'Sign in with it now.' })
      navigate('/sign-in', { replace: true })
    },
  })
  if (!token) {
    return (
      <AuthFrame title="This link is incomplete" intro="The address has no token, so there is nothing to spend.">
        <Button asChild variant="primary">
          <Link to="/forgot-password">Ask for a new link</Link>
        </Button>
      </AuthFrame>
    )
  }
  const shown = problem ?? (submit.error ? refusal(submit.error) : null)
  return (
    <AuthFrame title="Set a new password" intro={`At least ${PASSWORD_MIN} characters. No digit or symbol is required: length is what counts.`}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault()
          const found = passwordProblem(password) ?? (password === again ? null : 'the two passwords do not match')
          setProblem(found)
          if (!found) submit.mutate()
        }}
      >
        {shown ? (
          <Alert variant="destructive" icon={<TriangleAlert />}>
            {shown}
          </Alert>
        ) : null}
        <Field label="New password" htmlFor="rp-password">
          <Input id="rp-password" type="password" required autoFocus autoComplete="new-password" className="h-10" value={password} onChange={(e) => setPassword(e.target.value)} />
        </Field>
        <Field label="New password again" htmlFor="rp-again">
          <Input id="rp-again" type="password" required autoComplete="new-password" className="h-10" value={again} onChange={(e) => setAgain(e.target.value)} />
        </Field>
        <Button type="submit" variant="primary" size="lg" loading={submit.isPending} disabled={!password || !again}>
          Set the password
        </Button>
        {submit.error instanceof ApiError && submit.error.status === 400 ? (
          <Link to="/forgot-password" className="text-control">
            Ask for a new link
          </Link>
        ) : (
          <Link to="/sign-in" className="text-control">
            Back to sign in
          </Link>
        )}
      </form>
    </AuthFrame>
  )
}
