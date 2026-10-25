import { AlertTriangle, Copy, KeyRound, Lock, Plus, ShieldCheck, TriangleAlert, Upload } from 'lucide-react'
import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from '@/lib/router'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, Input, NativeSelect } from '@/components/ui/input'
import { Segmented } from '@/components/ui/segmented'
import { Switch } from '@/components/ui/switch'
import { Alert, Avatar, Table, Td, Th, Tr } from '@/components/ui/misc'
import { because } from '@/auth/client'
import { type Role } from '@/auth/api'
import { useAccount, useOrganizations } from '@/auth/session'
import { useExceptionAlerts, useIngestKeys, useProfile, useProjects, useReadKeys, useTeams } from '@/data/organization'
import { initialsOf, mayChange, mayHandOut, theLastOwner, useGrantRole, useInvite, useMembers, useRemoveMember, type Member, type Welcomed } from '@/data/members'
import { orgNameProblem, useOrgCard, useRenameOrg, type OrgCard } from '@/data/orgs'
import { stamp } from '@/lib/format'
import { useTheme, type ThemePref } from '@/lib/theme'
import { cn } from '@/lib/utils'

function H({ title, sub, children }: { title: string; sub?: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="flex flex-1 flex-col gap-1"><h2 className="text-title">{title}</h2>{sub ? <span className="text-control text-muted-foreground">{sub}</span> : null}</div>
      {children}
    </div>
  )
}
const Sub = ({ children }: { children: React.ReactNode }) => <h3 className="mt-2 text-[12px] font-semibold uppercase tracking-[.05em] text-muted-foreground">{children}</h3>
function Row({ title, sub, children }: { title: string; sub?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-border-subtle px-4 py-3 last:border-b-0">
      <span className="flex flex-[1_1_260px] flex-col"><span className="text-control font-semibold">{title}</span>{sub ? <span className="text-caption text-muted-foreground">{sub}</span> : null}</span>
      {children}
    </div>
  )
}
const saved = () => toast.success('Saved')
function Danger({ title, text, action, onClick }: { title: string; text: string; action: string; onClick: () => void }) {
  return (
    <Card className="flex flex-wrap items-center gap-4 border-destructive-border p-4">
      <div className="flex flex-[1_1_320px] flex-col gap-0.5"><span className="font-semibold">{title}</span><span className="text-control text-muted-foreground">{text}</span></div>
      <Button size="sm" className="text-destructive-text" onClick={onClick}>{action}</Button>
    </Card>
  )
}
function ConfirmDelete({ open, onOpenChange, name, what, body }: { open: boolean; onOpenChange: (o: boolean) => void; name: string; what: string; body: string }) {
  const [v, setV] = useState('')
  return (
    <Dialog open={open} onOpenChange={(o) => { onOpenChange(o); setV('') }}>
      <DialogContent role="alertdialog" title={`Delete ${name}?`} description={body}>
        <Field label={<>Type <span className="font-mono">{name}</span> to confirm</>} htmlFor="cd-in"><Input id="cd-in" mono value={v} onChange={(e) => setV(e.target.value)} autoFocus /></Field>
        <DialogFooter><Button onClick={() => onOpenChange(false)}>Cancel</Button><Button variant="destructive" disabled={v !== name} onClick={() => { onOpenChange(false); toast('Nothing was deleted', { description: `No route deletes a ${what} yet, so ${name} stays in place.` }) }}>Delete {what}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/* ---------------- Personal ---------------- */
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

export function Security() {
  const [twofa, setTwofa] = useState(false)
  const [setup, setSetup] = useState(false)
  const [code, setCode] = useState('')
  return (
    <>
      <H title="Login & security" sub="How you sign in, and where you are signed in." />
      <Card className="overflow-hidden">
        <Row title="Sign-in method" sub="Email and password. Single sign-on replaces it when the organization turns it on."><Badge variant="outline">Email and password</Badge></Row>
        <Row title="Two-factor authentication" sub="A code from an authenticator app at each sign-in.">{twofa ? <Badge variant="success"><ShieldCheck />On</Badge> : <><Badge variant="warning">Off</Badge><Button variant="primary" size="sm" onClick={() => setSetup(true)}>Set up</Button></>}</Row>
        <Row title="Recovery codes" sub={twofa ? '10 codes left.' : 'Available once two-factor is on.'}><Button size="sm" disabled={!twofa}>Show codes</Button></Row>
      </Card>
      <Card className="flex flex-col gap-3 p-4">
        <h3 className="text-heading">Password</h3>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(240px,100%),1fr))] gap-3">
          <Field label="Current password" htmlFor="pw-c"><Input id="pw-c" type="password" defaultValue="correct horse battery" autoComplete="current-password" /></Field>
          <Field label="New password" htmlFor="pw-n" hint="At least 12 characters."><Input id="pw-n" type="password" autoComplete="new-password" /></Field>
        </div>
        <div><Button variant="primary" size="sm" onClick={() => toast.success('Password changed', { description: 'Other sessions were signed out.' })}>Change password</Button></div>
      </Card>
      <Card className="overflow-hidden">
        <div className="flex items-center gap-3 px-4 py-3.5"><h3 className="flex-1 text-heading">Sessions</h3><Button size="sm" onClick={() => toast.success('2 other sessions signed out')}>Sign out other sessions</Button></div>
        <Table><thead><tr><Th>Device</Th><Th className="w-[140px]">Signed in</Th><Th className="w-[170px]">Last active</Th><Th className="w-[100px]"><span className="sr-only">Actions</span></Th></tr></thead>
          <tbody>
            <Tr><Td><span className="inline-flex items-center gap-2">Firefox on macOS<Badge variant="success">This device</Badge></span></Td><Td className="font-mono">2026-10-14</Td><Td className="font-mono">now</Td><Td /></Tr>
            <Tr><Td>Chrome on Linux</Td><Td className="font-mono">2026-10-02</Td><Td className="font-mono">2026-10-19 23:12</Td><Td numeric><Button variant="ghost" size="sm">Sign out</Button></Td></Tr>
            <Tr><Td>Safari on iOS</Td><Td className="font-mono">2026-09-21</Td><Td className="font-mono">2026-10-17 08:40</Td><Td numeric><Button variant="ghost" size="sm">Sign out</Button></Td></Tr>
          </tbody></Table>
      </Card>
      <Dialog open={setup} onOpenChange={setSetup}>
        <DialogContent title="Set up two-factor authentication" description="Scan the code with an authenticator app, then enter the 6 digits it shows.">
          <div className="flex items-center gap-4">
            <div role="img" aria-label="QR code" className="size-[132px] shrink-0 rounded-lg border bg-[repeating-conic-gradient(var(--foreground)_0_25%,var(--background)_0_50%)] bg-[length:16px_16px]" />
            <div className="flex min-w-0 flex-col gap-1.5"><span className="text-caption text-muted-foreground">Can&apos;t scan? Enter this key:</span><span className="break-all font-mono text-control">JBSW Y3DP EHPK 3PXP</span></div>
          </div>
          <Field label="Code" htmlFor="tf-c" hint="Any 6 digits: no route checks the code yet."><Input id="tf-c" mono inputMode="numeric" value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))} className="h-10 max-w-[180px] tracking-[.2em]" autoFocus /></Field>
          <DialogFooter><Button onClick={() => setSetup(false)}>Cancel</Button><Button variant="primary" disabled={code.length !== 6} onClick={() => { setTwofa(true); setSetup(false); toast.success('Two-factor authentication is on') }}>Verify and turn on</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function Preferences() {
  const me = useProfile()
  const { pref, setPref } = useTheme()
  const [prec, setPrec] = useState('ms')
  const [dens, setDens] = useState('compact')
  const [rows, setRows] = useState([
    ['Exception alerts for my teams', 'Rules set per project, e.g. new exception type', true, true],
    ['A service stops sending', 'No spans for 10 min', true, true],
    ['Status page incidents and updates', 'Posted by anyone in the workspace', true, false],
    ['Api keys about to expire', '7 days and 1 day before', true, true],
    ['Usage and billing', 'Admins only · 50 %, 80 % and 100 % of included', true, true],
    ['Members join or leave', 'Admins only', true, false],
  ] as [string, string, boolean, boolean][])
  const flip = (i: number, j: 2 | 3) => setRows((r) => r.map((x, k) => (k === i ? (Object.assign([...x], { [j]: !x[j] }) as typeof x) : x)))
  return (
    <>
      <H title="Preferences" sub="Only for you, on every device." />
      <Sub>Display</Sub>
      <Card className="overflow-hidden">
        <Row title="Theme" sub="Follows your system unless you pick one."><Segmented label="Theme" value={pref} onChange={(v: ThemePref) => setPref(v)} options={[{ value: 'system', label: 'System' }, { value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }]} /></Row>
        <Row title="Time zone" sub="Used for every timestamp. Raw values stay in UTC nanoseconds."><NativeSelect className="w-[240px]" aria-label="Time zone" defaultValue="browser"><option value="browser">Browser · Europe/Paris</option><option>UTC</option><option>America/New_York</option></NativeSelect></Row>
        <Row title="Timestamp precision" sub="In lists. Details always show the full value."><Segmented label="Precision" value={prec} onChange={setPrec} options={['ms', 'µs', 'ns'].map((v) => ({ value: v, label: v }))} /></Row>
        <Row title="Row density" sub="Traces, logs and tables."><Segmented label="Density" value={dens} onChange={setDens} options={[{ value: 'compact', label: 'Compact' }, { value: 'comfy', label: 'Comfortable' }]} /></Row>
        <Row title="Start page" sub="What opens after sign-in."><NativeSelect className="w-[200px]" aria-label="Start page"><option>Overview</option><option>Services</option><option>Traces</option></NativeSelect></Row>
      </Card>
      <Sub>Notifications</Sub>
      <Card className="overflow-hidden">
        <Row title="Email" sub={me.email}><Switch defaultChecked aria-label="Email notifications" /></Row>
        <Row title="Weekly digest" sub="Monday 09:00 · errors, slowest services, usage"><Switch aria-label="Weekly digest" /></Row>
      </Card>
      <Card className="overflow-x-auto">
        <Table aria-label="What to notify me about">
          <thead><tr><Th>Notify me when</Th><Th numeric className="w-[90px]">In-app</Th><Th numeric className="w-[90px]">Email</Th></tr></thead>
          <tbody>{rows.map((r, i) => (
            <Tr key={r[0]}><Td className="whitespace-normal py-2"><div className="flex flex-col gap-0.5"><span className="text-control font-semibold">{r[0]}</span><span className="text-caption text-muted-foreground">{r[1]}</span></div></Td>
              <Td numeric><Switch checked={r[2]} onCheckedChange={() => flip(i, 2)} aria-label={`In-app: ${r[0]}`} /></Td><Td numeric><Switch checked={r[3]} onCheckedChange={() => flip(i, 3)} aria-label={`Email: ${r[0]}`} /></Td></Tr>
          ))}</tbody>
        </Table>
      </Card>
      <div><Button variant="primary" size="sm" onClick={saved}>Save</Button></div>
    </>
  )
}

/* ---------------- Organization ---------------- */
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

export function Members() {
  const { active } = useOrganizations()
  const me = useAccount()
  const { members, nothing } = useMembers()
  const grant = useGrantRole()
  const remove = useRemoveMember()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [invite, setInvite] = useState(false)
  const [leaving, setLeaving] = useState(false)
  const mine = active?.role
  const roll = members ?? []
  const administered = roll.filter((m) => m.role !== 'member').length

  const change = (member: Member, role: Role) =>
    grant.mutate({ account: member.account, role }, { onSuccess: saved, onError: (error) => toast.error(because(error, 'The role was not changed')) })

  /* Removing your own membership is leaving, and every screen behind this one is then reading an
     organisation you no longer belong to. So the cache goes and the browser starts again. */
  const drop = (member: Member) =>
    remove.mutate(member.account, {
      onSuccess: () => {
        if (member.account !== me?.account) {
          toast(`${member.email} is no longer a member`)
          return
        }
        setLeaving(false)
        client.clear()
        navigate('/', { replace: true })
      },
      onError: (error) => toast.error(because(error, 'Nobody was removed')),
    })

  return (
    <>
      <H title="Members" sub={members ? `${roll.length} ${roll.length === 1 ? 'member' : 'members'} · ${administered} can administer it` : 'Who belongs to the organization.'}>
        {mine === 'owner' || mine === 'admin' ? <Button variant="primary" onClick={() => setInvite(true)}><Plus />Invite member</Button> : null}
      </H>
      <Card className="overflow-x-auto">
        <Table aria-label="Members">
          <thead><tr><Th>Member</Th><Th className="w-[160px]">Role</Th><Th className="w-[190px]">Joined</Th><Th className="w-[110px]"><span className="sr-only">Actions</span></Th></tr></thead>
          <tbody>{roll.map((m) => {
            const you = m.account === me?.account
            const last = theLastOwner(roll, m)
            const editable = mayChange(mine, m.role) && !last
            return (
              <Tr key={m.account}>
                <Td className="h-11"><span className="inline-flex items-center gap-2.5"><Avatar initials={initialsOf(m.email)} /><span className="font-semibold">{m.email}</span>{you ? <Badge variant="outline">you</Badge> : null}</span></Td>
                <Td>{editable ? (
                  <NativeSelect aria-label={`Role for ${m.email}`} className="h-7 w-[130px]" value={m.role} disabled={grant.isPending} onChange={(e) => change(m, e.target.value as Role)}>
                    {mayHandOut(mine).map((role) => <option key={role} value={role}>{role}</option>)}
                  </NativeSelect>
                ) : (
                  <span className="inline-flex items-center gap-1.5 text-control text-muted-foreground"><Lock className="size-3" aria-hidden />{m.role}{last ? ' · last owner' : null}</span>
                )}</Td>
                <Td className="font-mono text-control">{stamp(m.joined_at_ms)}</Td>
                <Td numeric>{editable ? <Button variant="ghost" size="sm" className="text-destructive-text" disabled={remove.isPending} onClick={() => (you ? setLeaving(true) : drop(m))}>{you ? 'Leave' : 'Remove'}</Button> : null}</Td>
              </Tr>
            )
          })}</tbody>
        </Table>
        {roll.length === 0 ? <p className="m-0 px-4 py-3 text-control text-muted-foreground">{members ? 'Nobody belongs to this organization.' : nothing}</p> : null}
      </Card>
      <p className="m-0 text-caption text-muted-foreground">Only an owner hands out the owner role or changes somebody who holds it, and an organization keeps at least one owner — so the last one stays until it has handed the role on. Somebody who has lost their password asks for a link on the sign-in screen.</p>
      <InviteMember open={invite} onOpenChange={setInvite} mine={mine} />
      <Dialog open={leaving} onOpenChange={setLeaving}>
        <DialogContent role="alertdialog" title={`Leave ${active?.name ?? 'the organization'}?`} description="You lose every screen of it at once. Somebody who stays has to invite you back.">
          <DialogFooter><Button onClick={() => setLeaving(false)}>Stay</Button><Button variant="destructive" disabled={remove.isPending} onClick={() => { const mineRow = roll.find((m) => m.account === me?.account); if (mineRow) drop(mineRow) }}>Leave</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function InviteMember({ open, onOpenChange, mine }: { open: boolean; onOpenChange: (o: boolean) => void; mine: Role | undefined }) {
  const invite = useInvite()
  const [email, setEmail] = useState('')
  const [role, setRole] = useState<Role>('member')
  const [welcomed, setWelcomed] = useState<Welcomed | null>(null)
  /* The server hands back a token, not a link: the path that spends it belongs to this app, and
     the server does not know what address the browser reached it on. */
  const link = welcomed?.invite_token ? `${window.location.origin}/reset?token=${welcomed.invite_token}` : null

  const close = (o: boolean) => {
    onOpenChange(o)
    if (!o) { setWelcomed(null); setEmail(''); setRole('member') }
  }

  const send = (e: React.FormEvent) => {
    e.preventDefault()
    invite.mutate({ email, role }, { onSuccess: setWelcomed, onError: (error) => toast.error(because(error, 'Nobody was invited')) })
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent
        title={welcomed ? `${welcomed.member.email} belongs to the organization` : 'Invite member'}
        description={welcomed ? (link ? `Role: ${welcomed.member.role}. Send the link through your own channel.` : `Role: ${welcomed.member.role}. It signs in with the password it already has.`) : 'An address that has no account here gets one, and a link that sets its password once.'}
      >
        {welcomed ? (
          <>
            {link ? (
              <>
                <Alert variant="warning" icon={<AlertTriangle />}>This link is shown once and can be spent once. Copy it before closing.</Alert>
                <div className="flex gap-2"><Input readOnly mono value={link} className="bg-muted text-[12px]" aria-label="Invitation link" /><Button variant="primary" onClick={() => { void navigator.clipboard?.writeText(link); toast.success('Copied') }}><Copy />Copy</Button></div>
              </>
            ) : (
              <Alert variant="info" icon={<ShieldCheck />}>An account already carried this address, so nothing here can set a password for it.</Alert>
            )}
            <DialogFooter><Button onClick={() => close(false)}>Done</Button></DialogFooter>
          </>
        ) : (
          <form className="flex flex-col gap-4" onSubmit={send}>
            <Field label="Email" htmlFor="iv-e"><Input id="iv-e" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="somebody@example.com" autoFocus /></Field>
            <Field label="Role"><div><Segmented label="Role" value={role} onChange={(chosen) => setRole(chosen as Role)} options={mayHandOut(mine).map((one) => ({ value: one, label: one }))} /></div></Field>
            <span className="text-caption text-muted-foreground">{role === 'owner' ? 'Answers for the organization: everything an administrator does, and the owner role itself.' : role === 'admin' ? 'Also manages members, projects, keys and billing.' : 'Reads traces, logs and metrics, and posts incidents.'}</span>
            <DialogFooter><Button type="button" onClick={() => close(false)}>Cancel</Button><Button variant="primary" type="submit" disabled={email.trim().length === 0 || invite.isPending}>Invite</Button></DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}

export function Teams() {
  const projects = useProjects()
  const [teams, setTeams] = useState(useTeams())
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('Platform')
  return (
    <>
      <H title="Teams" sub="A team groups members, owns projects and receives their exception alerts."><Button variant="primary" size="sm" onClick={() => setOpen(true)}><Plus />New team</Button></H>
      <Card className="overflow-x-auto">
        <Table aria-label="Teams"><thead><tr><Th>Team</Th><Th numeric className="w-[100px]">Members</Th><Th className="w-[220px]">Projects</Th><Th className="w-[180px]">Alerts go to</Th><Th className="w-[90px]"><span className="sr-only">Actions</span></Th></tr></thead>
          <tbody>{teams.map((t) => (
            <Tr key={t.name}><Td className="h-11"><span className="inline-flex items-center gap-2.5"><span className="inline-flex size-7 items-center justify-center rounded-md bg-muted text-[11px] font-bold text-muted-foreground">{t.initials}</span><span className="font-semibold">{t.name}</span></span></Td><Td numeric>{t.members}</Td><Td className="text-control">{t.projects}</Td><Td className="font-mono text-control text-muted-foreground">{t.channel}</Td><Td numeric><Button variant="ghost" size="sm">Manage</Button></Td></Tr>
          ))}</tbody></Table>
      </Card>
      <p className="m-0 text-caption text-muted-foreground">Teams decide who is alerted and who owns what. Admins see everything.</p>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent size="md" title="New team" description="A team owns projects and receives their exception alerts.">
          <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); setTeams((t) => [...t, { name, initials: name.slice(0, 2).toUpperCase(), members: 2, projects: 'storefront', channel: `#alerts-${name.toLowerCase()}` }]); setOpen(false); toast.success(`Team ${name} created`) }}>
            <Field label="Name" htmlFor="nt-n"><Input id="nt-n" value={name} onChange={(e) => setName(e.target.value)} autoFocus /></Field>
            <Field label="Projects it owns" hint="A project has one owner team."><NativeSelect>{projects.map((p) => <option key={p.name}>{p.name}</option>)}</NativeSelect></Field>
            <Field label="Alerts go to"><NativeSelect><option>Slack · #alerts-{name.toLowerCase() || 'team'}</option><option>Email only</option></NativeSelect></Field>
            <DialogFooter><Button type="button" onClick={() => setOpen(false)}>Cancel</Button><Button variant="primary" type="submit">Create team</Button></DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function Projects() {
  const [list, setList] = useState(useProjects())
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('mobile-app')
  return (
    <>
      <H title="Projects" sub="A project is a named source of telemetry with its own api keys."><Button size="sm" onClick={() => setOpen(true)}><Plus />New project</Button></H>
      <Card className="overflow-x-auto">
        <Table><thead><tr><Th>Name</Th><Th className="w-[140px]">Team</Th><Th numeric className="w-[110px]">Api keys</Th><Th className="w-[130px]">Created</Th><Th className="w-[110px]"><span className="sr-only">Actions</span></Th></tr></thead>
          <tbody>{list.map((p) => <Tr key={p.name}><Td><Link to="/settings/project/general" className="font-semibold text-foreground">{p.name}</Link></Td><Td className="text-control">{p.team}</Td><Td numeric>{p.keys}</Td><Td className="font-mono text-control">{p.created}</Td><Td numeric><Button variant="ghost" size="sm" asChild><Link to="/settings/project/general">Settings</Link></Button></Td></Tr>)}</tbody></Table>
      </Card>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent title="New project" description="Issue its first ingest key right after.">
          <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); setList((l) => [...l, { name, team: 'Backend', keys: 0, created: '2026-10-24' }]); setOpen(false); toast.success(`Project ${name} created`) }}>
            <Field label="Name" htmlFor="np-n" hint="Lowercase letters, digits and dashes."><Input id="np-n" mono value={name} onChange={(e) => setName(e.target.value)} autoFocus /></Field>
            <DialogFooter><Button type="button" onClick={() => setOpen(false)}>Cancel</Button><Button variant="primary" type="submit">Create project</Button></DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}

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

function Meter({ label, used, total, pct, note }: { label: string; used: string; total: string; pct: number; note: string }) {
  return (
    <Card className="flex flex-col gap-2 p-3.5">
      <div className="flex items-baseline gap-2"><span className="flex-1 text-control font-semibold">{label}</span><span className="font-mono text-control">{used}</span><span className="text-caption text-muted-foreground">of {total}</span></div>
      <div className="relative h-2 rounded bg-muted"><div className="absolute inset-y-0 left-0 rounded bg-primary" style={{ width: `${pct}%` }} /></div>
      <span className="text-caption text-muted-foreground">{note}</span>
    </Card>
  )
}
export function Usage() {
  const [period, setPeriod] = useState('this')
  return (
    <>
      <H title="Usage" sub="October 1 – 31 · resets in 7 days. Self-hosted shows storage only."><Segmented label="Period" value={period} onChange={setPeriod} options={[{ value: 'this', label: 'This month' }, { value: 'last', label: 'Last month' }]} /></H>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
        <Meter label="Spans" used="41.2 M" total="75 M included" pct={55} note="55 % · projected 92 % at month end" />
        <Meter label="Log records" used="118 M" total="200 M included" pct={59} note="59 % · projected 98 %" />
        <Meter label="Metric points" used="22.4 M" total="100 M included" pct={22} note="22 % · projected 36 %" />
        <Meter label="Storage" used="186 GB" total="500 GB" pct={37} note="Driven by retention: 14 days on storefront" />
      </div>
      <Card className="overflow-x-auto">
        <Table aria-label="Usage by project"><thead><tr><Th>Project</Th><Th numeric>Spans</Th><Th numeric>Log records</Th><Th numeric>Metric points</Th><Th numeric>Storage</Th><Th numeric>Share</Th></tr></thead>
          <tbody>{[['storefront', '31.9 M', '84.0 M', '16.1 M', '131 GB', '71 %'], ['payments-eu', '7.6 M', '26.3 M', '5.2 M', '41 GB', '22 %'], ['data-pipeline', '1.7 M', '7.7 M', '1.1 M', '14 GB', '7 %']].map((r) => <Tr key={r[0]}><Td className="font-semibold">{r[0]}</Td>{r.slice(1).map((c, i) => <Td key={i} numeric>{c}</Td>)}</Tr>)}</tbody></Table>
      </Card>
      <p className="m-0 text-caption text-muted-foreground">Over the included amount, Cloud bills per million; set alerts and a hard limit in <Link to="/settings/billing">Billing</Link>.</p>
    </>
  )
}

export function Billing() {
  return (
    <>
      <H title="Billing" sub="Cloud · Team plan · renews Nov 1" />
      <Card className="flex flex-wrap items-center gap-4 p-4">
        <div className="flex flex-[1_1_280px] flex-col gap-0.5"><span className="text-control text-muted-foreground">Current plan</span><span className="text-heading">Team · €[PRICE] / month</span><span className="text-caption text-muted-foreground">75 M spans, 200 M log records, 100 M metric points included. 14-day retention.</span></div>
        <Button size="sm">Change plan</Button>
      </Card>
      <Card className="overflow-hidden">
        <Row title="Projected bill" sub="Based on usage so far this month."><span className="font-mono text-heading">€[PRICE]</span></Row>
        <Row title="Usage alert" sub="Email admins at 80 % of included."><Switch defaultChecked aria-label="Usage alert" /></Row>
        <Row title="Hard limit" sub="Ingest returns 429 above it; queries keep working."><NativeSelect className="w-[200px]" aria-label="Hard limit"><option>Off</option><option>150 % of included</option><option>200 % of included</option></NativeSelect></Row>
        <Row title="Payment method" sub="Visa ending 4242 · expires 08/28"><Button size="sm">Update</Button></Row>
      </Card>
      <Card className="overflow-x-auto">
        <Table aria-label="Invoices"><thead><tr><Th>Invoice</Th><Th className="w-[130px]">Date</Th><Th numeric className="w-[120px]">Amount</Th><Th className="w-[100px]">Status</Th><Th className="w-[100px]"><span className="sr-only">Download</span></Th></tr></thead>
          <tbody>{['CMT-2026-0010', 'CMT-2026-0009', 'CMT-2026-0008'].map((n, i) => <Tr key={n}><Td className="font-mono">{n}</Td><Td className="font-mono">2026-{String(10 - i).padStart(2, '0')}-01</Td><Td numeric>€[PRICE]</Td><Td><Badge variant="success">Paid</Badge></Td><Td numeric><Button variant="ghost" size="sm">PDF</Button></Td></Tr>)}</tbody></Table>
      </Card>
    </>
  )
}

/* ---------------- Project ---------------- */
export function ProjectGeneral() {
  const teams = useTeams()
  const [days, setDays] = useState('14')
  const [del, setDel] = useState(false)
  return (
    <>
      <H title="General" sub="Project storefront." />
      <Card className="flex flex-col gap-3.5 p-4">
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(260px,100%),1fr))] gap-3">
          <Field label="Project name" htmlFor="pg-n" hint="Renaming keeps keys and signals."><Input id="pg-n" mono defaultValue="storefront" /></Field>
          <Field label="Owner team" hint="Receives this project's exception alerts by default."><NativeSelect defaultValue="Backend">{teams.map((t) => <option key={t.name}>{t.name}</option>)}</NativeSelect></Field>
        </div>
        <span className="text-control"><span className="text-muted-foreground">Project id</span> <span className="font-mono">prj_01JAZ4K2W8</span></span>
        <div><Button variant="primary" size="sm" onClick={saved}>Save</Button></div>
      </Card>
      <Sub>Retention</Sub>
      <Card className="flex flex-col gap-2.5 p-4">
        <label className="text-control font-semibold" htmlFor="pg-r">Keep signals for</label>
        <div className="flex items-center gap-2"><div className="flex w-[140px] items-center gap-2 rounded-md border border-input bg-background px-2.5 shadow-sm"><input id="pg-r" type="number" value={days} onChange={(e) => setDays(e.target.value)} className="h-8 w-full bg-transparent font-mono outline-none" /><span className="text-control text-muted-foreground">days</span></div><Button size="sm" onClick={saved}>Save</Button></div>
        <span className="text-caption text-muted-foreground">Spans, log records and metric points of storefront older than this are deleted on the next cleanup.</span>
      </Card>
      <Sub>Export</Sub>
      <Card className="flex flex-wrap items-center gap-4 p-4"><div className="flex flex-[1_1_320px] flex-col gap-0.5"><span className="text-control font-semibold">Export signals</span><span className="text-control text-muted-foreground">Download a time range as OTLP JSON. Large ranges run in the background.</span></div><Button size="sm" onClick={() => toast.success('Export started', { description: 'You get a notification when the file is ready.' })}>New export</Button></Card>
      <Danger title="Delete storefront" text="Deletes its signals, api keys and alerts. Other projects are not touched." action="Delete project" onClick={() => setDel(true)} />
      <ConfirmDelete open={del} onOpenChange={setDel} name="storefront" what="project" body="Its 6.5 M signals, 3 api keys and 4 alerts are deleted." />
    </>
  )
}

export function Alerts() {
  const [rules, setRules] = useState(useExceptionAlerts())
  const [open, setOpen] = useState(false)
  const [cond, setCond] = useState('rate')
  const [thr, setThr] = useState('2')
  return (
    <>
      <H title="Exception alerts" sub="Rules on the exceptions recorded in storefront's spans and logs."><Button variant="primary" size="sm" onClick={() => setOpen(true)}><Plus />New alert</Button></H>
      <Card className="overflow-x-auto">
        <Table aria-label="Alert rules"><thead><tr><Th>Rule</Th><Th className="w-[230px]">Notify</Th><Th className="w-[130px]">Last triggered</Th><Th numeric className="w-[70px]">On</Th></tr></thead>
          <tbody>{rules.map((r, i) => (
            <Tr key={r.name}><Td className="whitespace-normal py-2"><div className="flex flex-col gap-0.5"><span className="text-control font-semibold">{r.name}</span><span className="text-caption text-muted-foreground">{r.cond}</span></div></Td><Td className="whitespace-normal text-control">{r.notify}</Td><Td className={cn('font-mono text-control', r.fired && 'text-warning-text', r.last === 'never' && 'text-muted-foreground')}>{r.last}</Td>
              <Td numeric><Switch checked={r.on} onCheckedChange={(v) => setRules((x) => x.map((y, k) => (k === i ? { ...y, on: v } : y)))} aria-label={`Enable ${r.name}`} /></Td></Tr>
          ))}</tbody></Table>
      </Card>
      <Card className="overflow-hidden">
        <div className="border-b px-4 py-3 text-control font-semibold">Recent triggers</div>
        {[['PspTimeoutError · checkout-api', 'New exception type · 412 times'], ['checkout-api error rate 4.1 %', 'Error rate above 2 %']].map(([a, b]) => (
          <div key={a} className="flex items-center gap-2.5 border-b border-border-subtle px-4 py-2.5 text-control last:border-b-0"><TriangleAlert className="size-3.5 text-warning-text" aria-hidden /><span className="flex-1 font-mono">{a}</span><span className="text-muted-foreground max-md:hidden">{b}</span><time className="font-mono text-muted-foreground">14:05</time><Link to="/services/checkout-api">Open</Link></div>
        ))}
      </Card>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent size="md" title="New exception alert" description="storefront · fires once, then again only after it resolves.">
          <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); setRules((r) => [...r, { name: cond === 'rate' ? `checkout-api error rate above ${thr} %` : 'New rule', cond: cond === 'rate' ? `Error rate of checkout-api above ${thr} % for 5 min` : 'Custom condition', notify: 'Backend · Slack #alerts-backend', last: 'never', on: true, fired: false }]); setOpen(false); toast.success('Alert created') }}>
            <Field label="When"><div><Segmented label="Condition" value={cond} onChange={setCond} options={[{ value: 'new', label: 'New exception type' }, { value: 'rate', label: 'Error rate' }, { value: 'spike', label: 'Spike' }, { value: 'silence', label: 'Silence' }]} /></div></Field>
            {cond === 'rate' ? (
              <div className="flex flex-wrap items-center gap-2 text-control">Error rate of<NativeSelect className="w-[170px] font-mono"><option>checkout-api</option><option>payments</option></NativeSelect>above<div className="flex w-20 items-center gap-1 rounded-md border border-input px-2"><input aria-label="Threshold" value={thr} onChange={(e) => setThr(e.target.value)} className="h-8 w-full bg-transparent font-mono outline-none" />%</div>for 5 min</div>
            ) : null}
            {cond === 'rate' && Number(thr) < 4.1 ? <Alert variant="warning" icon={<TriangleAlert />}>Now: 4.1 %. This rule would fire right away.</Alert> : null}
            <Field label="Notify"><NativeSelect><option>Backend team · Slack #alerts-backend, email</option><option>Payments team · Slack #alerts-payments</option></NativeSelect></Field>
            <DialogFooter><Button type="button" onClick={() => setOpen(false)}>Cancel</Button><Button variant="primary" type="submit">Create alert</Button></DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function ApiKeys() {
  const readKeys = useReadKeys()
  const [keys, setKeys] = useState(useIngestKeys())
  const [issue, setIssue] = useState(false)
  const [name, setName] = useState('edge-collector')
  const [secret, setSecret] = useState<string | null>(null)
  const [revoke, setRevoke] = useState<string | null>(null)
  return (
    <>
      <H title="Api keys" sub="Keys for storefront. Each key is shown once, then only its prefix." />
      <div className="mt-1 flex flex-wrap items-center gap-3"><span className="flex flex-1 flex-col"><span className="font-semibold">Ingest keys <span className="font-mono font-normal text-muted-foreground">cmt_ik_</span></span><span className="text-control text-muted-foreground">Write only. Used by SDKs and collectors to send signals.</span></span><Button variant="primary" size="sm" onClick={() => setIssue(true)}><KeyRound />Issue ingest key</Button></div>
      <Card className="overflow-x-auto">
        <Table><thead><tr><Th>Name</Th><Th className="w-[200px]">Key</Th><Th className="w-[120px]">Created</Th><Th className="w-[170px]">Last used</Th><Th className="w-[100px]"><span className="sr-only">Actions</span></Th></tr></thead>
          <tbody>{keys.map((k) => (
            <Tr key={k.name}><Td className={cn('font-semibold', k.revoked && 'font-normal text-muted-foreground line-through')}>{k.name}</Td><Td className="font-mono">{k.prefix}<span className="text-faint-foreground">••••••</span></Td><Td className="font-mono">{k.created}</Td><Td className={cn('font-mono', k.used === 'never' && 'text-muted-foreground')}>{k.used}</Td>
              <Td numeric>{k.revoked ? <Badge variant="outline">Revoked</Badge> : <Button variant="ghost" size="sm" className="text-destructive-text" onClick={() => setRevoke(k.name)}>Revoke</Button>}</Td></Tr>
          ))}</tbody></Table>
      </Card>
      <div className="mt-2 flex flex-wrap items-center gap-3"><span className="flex flex-1 flex-col"><span className="font-semibold">Query keys <span className="font-mono font-normal text-muted-foreground">cmt_at_</span></span><span className="text-control text-muted-foreground">Read only. Used by scripts and dashboards to query this project.</span></span><Button size="sm"><Plus />Create query key</Button></div>
      <Card className="overflow-x-auto">
        <Table><thead><tr><Th>Name</Th><Th className="w-[200px]">Key</Th><Th className="w-[160px]">Created by</Th><Th className="w-[120px]">Created</Th><Th className="w-[170px]">Last used</Th></tr></thead>
          <tbody>{readKeys.map((k) => <Tr key={k.name}><Td className="font-semibold">{k.name}</Td><Td className="font-mono">{k.prefix}<span className="text-faint-foreground">••••••</span></Td><Td className="text-control">{k.by}</Td><Td className="font-mono">{k.created}</Td><Td className="font-mono">{k.used}</Td></Tr>)}</tbody></Table>
      </Card>
      <Dialog open={issue} onOpenChange={(o) => { setIssue(o); if (!o) setSecret(null) }}>
        <DialogContent size={secret ? 'md' : 'sm'} title={secret ? 'Ingest key for storefront' : 'Issue ingest key'} description={secret ? name : 'The key is shown once after this step.'}>
          {secret ? (
            <>
              <Alert variant="warning" icon={<AlertTriangle />}>Copy this key now. It is shown once and cannot be retrieved later.</Alert>
              <div className="flex gap-2"><Input readOnly mono value={secret} className="bg-muted text-[12px]" aria-label="New ingest key" /><Button onClick={() => { void navigator.clipboard?.writeText(secret); toast.success('Key copied') }}><Copy />Copy</Button></div>
              <pre className="m-0 overflow-x-auto rounded-md border bg-muted px-3 py-2.5 font-mono text-[12px]">export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer {secret.slice(0, 11)}…"</pre>
              <DialogFooter><Button variant="primary" onClick={() => { setIssue(false); setSecret(null) }}>Done</Button></DialogFooter>
            </>
          ) : (
            <form className="flex flex-col gap-4" onSubmit={(e) => { e.preventDefault(); const s = `cmt_ik_${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2, 10)}`; setSecret(s); setKeys((k) => [{ name, prefix: s.slice(0, 11), created: '2026-10-24', used: 'never', revoked: false }, ...k]) }}>
              <Field label="Name" htmlFor="ik-n" hint="Where the key will live, so you know what breaks when you revoke it."><Input id="ik-n" value={name} onChange={(e) => setName(e.target.value)} autoFocus /></Field>
              <DialogFooter><Button type="button" onClick={() => setIssue(false)}>Cancel</Button><Button variant="primary" type="submit">Issue key</Button></DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>
      <Dialog open={!!revoke} onOpenChange={(o) => !o && setRevoke(null)}>
        <DialogContent role="alertdialog" title={`Revoke ${revoke}?`} description="SDKs sending with this key are rejected from now on. This cannot be undone.">
          <DialogFooter><Button onClick={() => setRevoke(null)}>Cancel</Button><Button variant="destructive" onClick={() => { setKeys((k) => k.map((x) => (x.name === revoke ? { ...x, revoked: true } : x))); setRevoke(null); toast.success('Key revoked') }}>Revoke key</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
