import { Copy, Upload } from 'lucide-react'
import { useRef, useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input } from '@/components/ui/input'
import { Avatar } from '@/components/ui/misc'
import { because } from '@/auth/client'
import { useAccount } from '@/auth/session'
import { accountNameProblem, initialsOf, nameOf, photoProblem, useMyStanding, usePutPhoto, useRemovePhoto, useRenameMe } from '@/data/profile'
import { stamp } from '@/lib/format'
import { cn } from '@/lib/utils'
import { H } from './parts'

/* The facts rather than the fields: the address signs you in and no route moves it yet, the id is
   what a support question is about, and the three below it are the organisation's answer about
   you rather than yours. */
function Standing() {
  const account = useAccount()
  const { active, mine, noRoll, teams, noTeams } = useMyStanding()
  const muted = (sentence: string) => <span className="text-muted-foreground">{sentence}</span>

  const rows: [string, React.ReactNode][] = [
    ['Email', <span className="font-mono">{account?.email ?? '—'}</span>],
    [
      'Account id',
      account ? (
        <span className="inline-flex items-center gap-2 font-mono">
          {account.account}
          <Button variant="ghost" size="icon-sm" aria-label="Copy id" onClick={() => { void navigator.clipboard?.writeText(account.account); toast.success('Copied') }}><Copy /></Button>
        </span>
      ) : muted('—'),
    ],
    [active ? `Role in ${active.name}` : 'Role', active ? <Badge variant="accent">{active.role}</Badge> : muted('This session belongs to no organization.')],
    ['Teams', teams === undefined ? muted(noTeams) : teams.length > 0 ? teams.map((team) => team.name).join(', ') : muted('You sit in none of them yet.')],
    ['Member since', mine ? <span className="font-mono">{stamp(mine.joined_at_ms)}</span> : muted(noRoll)],
  ]

  return (
    <Card><dl className="m-0 grid grid-cols-[200px_minmax(0,1fr)] text-control">
      {rows.map(([k, v], i, a) => (
        <div key={k} className="contents"><dt className={cn('px-4 py-3 text-muted-foreground', i < a.length - 1 && 'border-b border-border-subtle')}>{k}</dt><dd className={cn('m-0 px-4 py-2.5', i < a.length - 1 && 'border-b border-border-subtle')}>{v}</dd></div>
      ))}
    </dl></Card>
  )
}

/* A hidden input and a button, because a file input styled to look like the rest of the screen
   is a fight nobody wins. */
function Face() {
  const account = useAccount()
  const picker = useRef<HTMLInputElement>(null)
  const put = usePutPhoto()
  const drop = useRemovePhoto()
  const shown = account ? nameOf(account) : '—'
  const busy = put.isPending || drop.isPending

  const pick = (event: React.ChangeEvent<HTMLInputElement>) => {
    const chosen = event.target.files?.[0]
    /* Cleared so choosing the same file twice is two events rather than one. */
    event.target.value = ''
    if (!chosen) return
    const problem = photoProblem(chosen)
    if (problem) {
      toast.error(problem)
      return
    }
    put.mutate(chosen, {
      onSuccess: () => toast.success('Saved'),
      onError: (error) => toast.error(because(error, 'Your photo was not kept')),
    })
  }

  const remove = () =>
    drop.mutate(undefined, {
      onSuccess: () => toast.success('Your initials are back'),
      onError: (error) => toast.error(because(error, 'Your photo was not removed')),
    })

  return (
    <div className="flex flex-wrap items-center gap-3">
      <Avatar initials={initialsOf(shown)} src={account?.photo} className="size-14 text-[20px]" />
      <span className="flex min-w-0 flex-col leading-5"><span className="truncate text-heading">{shown}</span><span className="truncate text-caption text-muted-foreground">{account?.name ? account.email : 'No name yet, so your address is what names you.'}</span></span>
      <span className="flex flex-wrap items-center gap-2">
        <input ref={picker} type="file" accept="image/png,image/jpeg,image/webp" className="hidden" onChange={pick} />
        <Button size="sm" disabled={!account || busy} onClick={() => picker.current?.click()}><Upload />{account?.photo ? 'Replace photo' : 'Upload photo'}</Button>
        {account?.photo ? <Button size="sm" variant="ghost" disabled={busy} onClick={remove}>Remove</Button> : null}
        <span className="text-caption text-muted-foreground">PNG, JPEG or WebP, up to 1 MB.</span>
      </span>
    </div>
  )
}

export function Profile() {
  const account = useAccount()
  const rename = useRenameMe()
  /* Null means the field is still showing what the server holds, so a name written in another tab
     reaches it; anything else is what is being typed. */
  const [typed, setTyped] = useState<string | null>(null)
  const written = account?.name ?? ''
  const name = typed ?? written
  const problem = typed === null ? null : accountNameProblem(typed)
  const unchanged = name.trim() === written

  const save = () =>
    rename.mutate(name, {
      onSuccess: (standing) => {
        setTyped(null)
        toast.success(standing.name ? 'Saved' : 'Your name is your address again')
      },
      onError: (error) => toast.error(because(error, 'Your name was not changed')),
    })

  return (
    <>
      <H title="Profile" sub="The name and face you appear under, and what your organization holds about you." />
      <Card className="flex flex-col gap-3.5 p-4">
        <Face />
        <Field label="Name" htmlFor="pf-n" hint="Shown in the top bar and in the members table. Empty it to go back to your address.">
          <div className="flex max-w-[420px] gap-2">
            <Input id="pf-n" value={name} disabled={!account} aria-invalid={problem ? true : undefined} placeholder={account?.email ?? ''} onChange={(e) => setTyped(e.target.value)} />
            <Button size="sm" className="h-8" variant="primary" disabled={!account || unchanged || Boolean(problem) || rename.isPending} onClick={save}>Save</Button>
          </div>
        </Field>
        {problem ? <span className="text-caption text-destructive-text">{problem}</span> : null}
      </Card>
      <Standing />
    </>
  )
}
