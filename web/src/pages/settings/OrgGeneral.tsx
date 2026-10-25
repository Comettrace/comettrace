import { Copy } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input } from '@/components/ui/input'
import { because } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { orgNameProblem, useOrgCard, useRenameOrg, type OrgCard } from '@/data/orgs'
import { stamp } from '@/lib/format'
import { cn } from '@/lib/utils'
import { ConfirmDelete, Danger, H } from './parts'

function Identity({ card }: { card: OrgCard }) {
  const rows: [string, React.ReactNode][] = [
    ['Organization id', <span className="inline-flex items-center gap-2 font-mono">{card.id}<Button variant="ghost" size="icon-sm" aria-label="Copy id" onClick={() => { void navigator.clipboard?.writeText(card.id); toast.success('Copied') }}><Copy /></Button></span>],
    ['Created', <span className="font-mono">{stamp(card.created_at_ms)}</span>],
    ['Plan', card.plan],
    ['Version', <span className="font-mono">{card.version}</span>],
  ]
  return (
    <Card><dl className="m-0 grid grid-cols-[200px_minmax(0,1fr)] text-control">
      {rows.map(([k, v], i, a) => (
        <div key={k} className="contents"><dt className={cn('px-4 py-3 text-muted-foreground', i < a.length - 1 && 'border-b border-border-subtle')}>{k}</dt><dd className={cn('m-0 px-4 py-2.5', i < a.length - 1 && 'border-b border-border-subtle')}>{v}</dd></div>
      ))}
    </dl></Card>
  )
}

export function OrgGeneral() {
  const { active } = useOrganizations()
  const { card, nothing } = useOrgCard()
  const rename = useRenameOrg()
  /* Null means the field is still showing what the server holds, so a rename made elsewhere
     reaches it; anything else is what is being typed. */
  const [typed, setTyped] = useState<string | null>(null)
  const [del, setDel] = useState(false)
  const name = typed ?? card?.name ?? ''
  const problem = typed === null ? null : orgNameProblem(typed)
  const mayChange = active?.role === 'owner' || active?.role === 'admin'
  const unchanged = name.trim() === (card?.name ?? '')

  const save = () =>
    rename.mutate(name, {
      onSuccess: () => {
        setTyped(null)
        toast.success('Saved')
      },
      onError: (error) => toast.error(because(error, 'The organization was not renamed')),
    })

  return (
    <>
      <H title="General" sub={card ? `The organization: ${card.name}.` : 'The organization.'} />
      <Card className="flex flex-col gap-3.5 p-4">
        <Field label="Organization name" htmlFor="og-n" hint={mayChange ? 'Shown in the top bar and on invitations.' : 'Only an owner or an administrator can change this.'}>
          <div className="flex max-w-[420px] gap-2">
            <Input id="og-n" mono value={name} disabled={!card || !mayChange} aria-invalid={problem ? true : undefined} onChange={(e) => setTyped(e.target.value)} />
            <Button size="sm" className="h-8" variant="primary" disabled={!card || !mayChange || unchanged || Boolean(problem) || rename.isPending} onClick={save}>Save</Button>
          </div>
        </Field>
        {problem ? <span className="text-caption text-destructive-text">{problem}</span> : null}
      </Card>
      {card ? (
        <>
          <Identity card={card} />
          <Danger title={`Delete ${card.name}`} text="Deletes every project, signal, member and key of the organization. Export first if you need anything." action="Delete organization" onClick={() => setDel(true)} />
          <ConfirmDelete open={del} onOpenChange={setDel} name={card.name} what="organization" body="Every project, signal, member, team, key and alert is deleted. This cannot be undone." />
        </>
      ) : (
        <Card className="px-4 py-3 text-control text-muted-foreground">{nothing}</Card>
      )}
    </>
  )
}
