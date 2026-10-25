import { useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { H, Sub } from './parts'

export function Integrations() {
  const [on, setOn] = useState<Record<string, boolean>>({ Slack: true, Email: true, Webhooks: true })
  const card = (m: string, n: string, d: string) => (
    <Card key={n} className="flex flex-col gap-2.5 p-3.5">
      <div className="flex items-center gap-2.5"><span className="inline-flex size-7 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground">{m}</span><span className="flex-1 font-semibold">{n}</span>{on[n] ? <Badge variant="success">Connected</Badge> : <Badge variant="outline">Not connected</Badge>}</div>
      <span className="flex-1 text-control text-muted-foreground">{d}</span>
      <div><Button size="sm" onClick={() => { setOn((o) => ({ ...o, [n]: !o[n] })); toast.success(on[n] ? `${n} disconnected` : `${n} connected`) }}>{on[n] ? 'Configure' : 'Connect'}</Button></div>
    </Card>
  )
  return (
    <>
      <H title="Integrations" sub="Where alerts go, and how people sign in." />
      <Sub>Alert channels</Sub>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        {card('SL', 'Slack', 'Post exception alerts and status page incidents to channels.')}
        {card('@', 'Email', 'Uses the SMTP settings of the container. Cloud sends for you.')}
        {card('WH', 'Webhooks', 'Signed JSON to your endpoints.')}
        {card('PD', 'PagerDuty', 'Page the on-call person for critical alerts.')}
        {card('MT', 'Microsoft Teams', 'Post alerts to a Teams channel.')}
      </div>
      <Sub>Code and sign-in</Sub>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
        {card('GH', 'GitHub', 'Link stack frames in exceptions to the right line of your repository.')}
        {card('ID', 'Single sign-on', 'OpenID Connect. Local accounts stay on for admins.')}
      </div>
    </>
  )
}
