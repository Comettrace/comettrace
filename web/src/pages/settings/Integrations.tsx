import { useQuery } from '@tanstack/react-query'
import { methodsQuery } from '@/auth/api'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import { useWired } from '@/data/integrations'
import { H, Sub } from './parts'

/* Every one of these is set in the deployment's configuration file, the way the identity
   provider and the mail relay already were, so there is nothing on this screen to press. Its job
   is to say what is wired up and name the setting that wired it — which is the one thing an
   operator cannot read off the screens that use it. */
type Wiring = {
  mark: string
  name: string
  what: string
  on: boolean
  setting: string
  detail?: string
}

function Tile({ wiring }: { wiring: Wiring }) {
  return (
    <Card className="flex flex-col gap-2.5 p-3.5">
      <div className="flex items-center gap-2.5">
        <span className="inline-flex size-7 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground" aria-hidden>{wiring.mark}</span>
        <span className="flex-1 font-semibold">{wiring.name}</span>
        {wiring.on ? <Badge variant="success">Configured</Badge> : <Badge variant="outline">Not configured</Badge>}
      </div>
      <span className="flex-1 text-control text-muted-foreground">{wiring.what}</span>
      {wiring.on && wiring.detail ? <span className="font-mono text-caption text-muted-foreground">{wiring.detail}</span> : null}
      <span className="font-mono text-caption text-faint-foreground">{wiring.setting}</span>
    </Card>
  )
}

export function Integrations() {
  const { wired, loading, unreachable } = useWired()
  const methods = useQuery(methodsQuery).data

  const tracking: Wiring[] = [
    {
      mark: 'LI',
      name: 'Linear',
      what: 'Open a Linear issue from an exception.',
      on: Boolean(wired?.linear),
      setting: 'COMETTRACE_LINEAR__TOKEN',
    },
    {
      mark: 'GH',
      name: 'GitHub',
      what: 'Open an issue, and link a stack frame to the line of the repository it came from.',
      on: Boolean(wired?.github),
      detail: wired?.github?.repository,
      setting: 'COMETTRACE_GITHUB__TOKEN, COMETTRACE_GITHUB__REPOSITORY',
    },
    {
      mark: 'JI',
      name: 'Jira',
      what: 'Open a Jira Cloud issue, and close ours when it is done.',
      on: Boolean(wired?.jira),
      detail: wired?.jira ? `${wired.jira.base_url} · ${wired.jira.project_key}` : undefined,
      setting: 'COMETTRACE_JIRA__BASE_URL, __EMAIL, __TOKEN, __PROJECT_KEY',
    },
  ]

  const deployment: Wiring[] = [
    {
      mark: 'ID',
      name: 'Single sign-on',
      what: 'OpenID Connect. Local passwords stay on unless the deployment turns them off.',
      on: Boolean(methods?.oidc),
      setting: 'COMETTRACE_OIDC__ISSUER, __CLIENT_ID, __CLIENT_SECRET, __REDIRECT_URI',
    },
    {
      mark: '@',
      name: 'Email',
      what: 'The relay that sends a password reset link. Nothing else goes through it yet.',
      on: Boolean(wired?.mail),
      setting: 'COMETTRACE_MAIL__RELAY, __USERNAME, __PASSWORD, __FROM, __BASE_URL',
    },
  ]

  return (
    <>
      <H title="Integrations" sub="What this deployment is wired up to. Every one is set in its configuration file, not here." />
      {unreachable ? <Card className="px-4 py-3 text-control text-muted-foreground">The integrations could not be read.</Card> : null}
      <Sub>Project management</Sub>
      <p className="m-0 text-caption text-muted-foreground">Where an exception becomes a piece of work somebody is tracking. Nothing opens an issue yet: an exception has nowhere to live until a span carries its events, so these say what is wired up and wait.</p>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        {tracking.map((wiring) => <Tile key={wiring.name} wiring={wiring} />)}
      </div>
      <Sub>Sign-in and mail</Sub>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        {deployment.map((wiring) => <Tile key={wiring.name} wiring={wiring} />)}
      </div>
      {loading ? <p className="m-0 text-caption text-muted-foreground">Reading what is wired up…</p> : null}
    </>
  )
}
