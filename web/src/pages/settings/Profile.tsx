import { Upload } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input } from '@/components/ui/input'
import { Avatar } from '@/components/ui/misc'
import { useProfile } from '@/data/organization'
import { H, saved } from './parts'

export function Profile() {
  const me = useProfile()
  return (
    <>
      <H title="Profile" sub="How you appear to other members of acme-prod." />
      <Card className="flex flex-col gap-4 p-4">
        <div className="flex flex-wrap items-center gap-4"><Avatar initials={me.initials} className="size-14 text-[20px]" /><Button size="sm"><Upload />Upload photo</Button><span className="text-caption text-muted-foreground">PNG or JPG, square, up to 1 MB.</span></div>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(260px,100%),1fr))] gap-3">
          <Field label="Full name" htmlFor="pf-n"><Input id="pf-n" defaultValue={me.name} /></Field>
          <Field label="Display name" htmlFor="pf-d" hint="Shown on incidents and comments."><Input id="pf-d" defaultValue="Inès" /></Field>
        </div>
        <Field label="Email" htmlFor="pf-e" hint="We send a link to the new address; the change applies once you open it."><div className="flex max-w-[520px] gap-2"><Input id="pf-e" mono defaultValue={me.email} /><Button size="sm" className="h-8">Change</Button></div></Field>
        <div><Button variant="primary" size="sm" onClick={saved}>Save</Button></div>
      </Card>
      <Card><dl className="m-0 grid grid-cols-[200px_minmax(0,1fr)] text-control">
        <dt className="border-b border-border-subtle px-4 py-3 text-muted-foreground">Role in acme-prod</dt><dd className="m-0 border-b border-border-subtle px-4 py-3"><Badge variant="accent">admin</Badge></dd>
        <dt className="border-b border-border-subtle px-4 py-3 text-muted-foreground">Teams</dt><dd className="m-0 border-b border-border-subtle px-4 py-3">Backend, Payments</dd>
        <dt className="px-4 py-3 text-muted-foreground">Member since</dt><dd className="m-0 px-4 py-3 font-mono">2026-03-02</dd>
      </dl></Card>
    </>
  )
}
