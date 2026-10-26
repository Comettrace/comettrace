import { Copy, Upload } from 'lucide-react'
import { useRef, useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input } from '@/components/ui/input'
import { Avatar } from '@/components/ui/misc'
import { because } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { logoProblem, orgNameProblem, useOrgCard, usePutOrgLogo, useRemoveOrgLogo, useRenameOrg, type OrgCard } from '@/data/orgs'
import { initialsOf } from '@/data/profile'
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

/* A hidden input and a button, because a file input styled to look like the rest of the screen is
   a fight nobody wins. The mark is square-cornered rather than round: it is a logo somebody drew,
   not a face. */
function Mark({ card, mayChange }: { card: OrgCard | undefined; mayChange: boolean }) {
  const picker = useRef<HTMLInputElement>(null)
  const put = usePutOrgLogo()
  const drop = useRemoveOrgLogo()
  const busy = put.isPending || drop.isPending

  const pick = (event: React.ChangeEvent<HTMLInputElement>) => {
    const chosen = event.target.files?.[0]
    /* Cleared so choosing the same file twice is two events rather than one. */
    event.target.value = ''
    if (!chosen) return
    const problem = logoProblem(chosen)
    if (problem) {
      toast.error(problem)
      return
    }
    put.mutate(chosen, {
      onSuccess: () => toast.success('Saved'),
      onError: (error) => toast.error(because(error, 'The logo was not kept')),
    })
  }

  const remove = () =>
    drop.mutate(undefined, {
      onSuccess: () => toast.success('The initials are back'),
      onError: (error) => toast.error(because(error, 'The logo was not removed')),
    })

  return (
    <div className="flex flex-wrap items-center gap-3">
      <Avatar initials={initialsOf(card?.name ?? '?')} src={card?.logo} className="size-14 rounded-lg text-[20px]" />
      <span className="flex min-w-0 flex-1 flex-col leading-5">
        <span className="truncate text-heading">{card?.name ?? '—'}</span>
        <span className="truncate text-caption text-muted-foreground">{mayChange ? 'Shown wherever the organization is named. Without one, its initials stand in.' : 'Only an owner or an administrator can change this.'}</span>
      </span>
      <span className="flex flex-wrap items-center gap-2">
        <input ref={picker} type="file" accept="image/png,image/jpeg,image/webp" className="hidden" onChange={pick} />
        <Button size="sm" disabled={!card || !mayChange || busy} onClick={() => picker.current?.click()}><Upload />{card?.logo ? 'Replace logo' : 'Upload logo'}</Button>
        {card?.logo ? <Button size="sm" variant="ghost" disabled={!mayChange || busy} onClick={remove}>Remove</Button> : null}
        <span className="text-caption text-muted-foreground">PNG, JPEG or WebP, up to 1 MB.</span>
      </span>
    </div>
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
        <Mark card={card} mayChange={mayChange} />
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
