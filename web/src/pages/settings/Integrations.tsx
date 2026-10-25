import { useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import { methodsQuery } from '@/auth/api'
import { because } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { WHY, useConnect, useConnections, useDisconnect, useFilings, useWired, type Connected, type Routing } from '@/data/integrations'
import { stamp } from '@/lib/format'
import { Aim } from './aim'
import { H, Sub } from './parts'

/* Every one of these is registered by the deployment in its configuration file, the way the
   identity provider and the mail relay already were. What an organisation does on this screen is
   connect itself to one that is registered — so a tile has two states to tell apart, and they
   belong to two different people: an operator sets the application, an administrator connects. */
type Wiring = {
  mark: string
  name: string
  provider?: 'linear' | 'github' | 'jira'
  what: string
  on: boolean
  setting: string
  detail?: string
}

function Tile({
  wiring,
  connection,
  routing,
  mayConnect,
  busy,
  onConnect,
  onDisconnect,
}: {
  wiring: Wiring
  connection?: Connected
  routing?: Routing
  mayConnect: boolean
  busy: boolean
  onConnect: () => void
  onDisconnect: () => void
}) {
  return (
    <Card className="flex flex-col gap-2.5 p-3.5">
      <div className="flex items-center gap-2.5">
        <span className="inline-flex size-7 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground" aria-hidden>{wiring.mark}</span>
        <span className="flex-1 font-semibold">{wiring.name}</span>
        {connection ? <Badge variant="success">Connected</Badge> : wiring.on ? <Badge variant="outline">Not connected</Badge> : <Badge variant="outline">Not configured</Badge>}
      </div>
      <span className="flex-1 text-control text-muted-foreground">{wiring.what}</span>
      {connection ? (
        <>
          <span className="text-caption text-muted-foreground">
            <span className="font-semibold text-foreground">{connection.account_label}</span> · since {stamp(connection.connected_at_ms)}
          </span>
          {/* Where the organisation files by default. A project can differ, and says so on its
             own settings screen rather than here: this card is the organisation's. */}
          {routing ? <Aim routing={routing} mayChange={mayConnect} /> : null}
        </>
      ) : (
        <>
          {wiring.on && wiring.detail ? <span className="font-mono text-caption text-muted-foreground">{wiring.detail}</span> : null}
          <span className="font-mono text-caption text-faint-foreground">{wiring.setting}</span>
        </>
      )}
      {wiring.provider && wiring.on && mayConnect ? (
        <div>
          {connection ? (
            <Button size="sm" className="text-destructive-text" disabled={busy} onClick={onDisconnect}>Disconnect</Button>
          ) : (
            <Button size="sm" variant="primary" disabled={busy} onClick={onConnect}>Connect</Button>
          )}
        </div>
      ) : null}
    </Card>
  )
}

export function Integrations() {
  const { active } = useOrganizations()
  const { wired, loading, unreachable } = useWired()
  const methods = useQuery(methodsQuery).data
  const connections = useConnections()
  const { routings } = useFilings()
  const connect = useConnect()
  const disconnect = useDisconnect()
  const mayConnect = active?.role === 'owner' || active?.role === 'admin'
  const connected = (provider: string) => connections.find((one) => one.provider === provider)
  const aimed = (provider: string) => routings?.find((one) => one.provider === provider)

  /* The callback sends the browser back here with one word. Said once, then taken out of the url
     so a reload does not say it again. */
  useEffect(() => {
    const url = new URL(window.location.href)
    const failed = url.searchParams.get('failed')
    const done = url.searchParams.get('connected')
    if (!failed && !done) return
    if (done) toast.success(`${done} is connected`)
    else if (failed) toast.error(WHY[failed] ?? WHY.unavailable)
    url.searchParams.delete('failed')
    url.searchParams.delete('connected')
    window.history.replaceState(null, '', url.toString())
  }, [])

  const tracking: Wiring[] = [
    {
      mark: 'LI',
      name: 'Linear',
      provider: 'linear',
      what: 'Open a Linear issue from an exception.',
      on: Boolean(wired?.linear),
      setting: 'COMETTRACE_LINEAR__CLIENT_ID, __CLIENT_SECRET, __REDIRECT_URI',
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

  const tile = (wiring: Wiring) => (
    <Tile
      key={wiring.name}
      wiring={wiring}
      connection={wiring.provider ? connected(wiring.provider) : undefined}
      routing={wiring.provider ? aimed(wiring.provider) : undefined}
      mayConnect={mayConnect}
      busy={connect.isPending || disconnect.isPending}
      onConnect={() =>
        wiring.provider &&
        connect.mutate(wiring.provider, {
          onError: (error) => toast.error(because(error, `${wiring.name} could not be connected`)),
        })
      }
      onDisconnect={() =>
        wiring.provider &&
        disconnect.mutate(wiring.provider, {
          onSuccess: () => toast(`${wiring.name} is disconnected`),
          onError: (error) => toast.error(because(error, `${wiring.name} was not disconnected`)),
        })
      }
    />
  )

  return (
    <>
      <H title="Integrations" sub="An operator registers the application in this deployment's configuration; an administrator connects this organization to it." />
      {unreachable ? <Card className="px-4 py-3 text-control text-muted-foreground">The integrations could not be read.</Card> : null}
      <Sub>Project management</Sub>
      <p className="m-0 text-caption text-muted-foreground">Where an exception becomes a piece of work somebody is tracking. Nothing opens an issue yet: an exception has nowhere to live until a span carries its events, so a connection made here waits for that.</p>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">{tracking.map(tile)}</div>
      <Sub>Sign-in and mail</Sub>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">{deployment.map(tile)}</div>
      {loading ? <p className="m-0 text-caption text-muted-foreground">Reading what is wired up…</p> : null}
    </>
  )
}
