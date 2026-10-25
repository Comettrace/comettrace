import { Plus } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, Input, NativeSelect } from '@/components/ui/input'
import { Table, Td, Th, Tr } from '@/components/ui/misc'
import { useProjects, useTeams } from '@/data/organization'
import { H } from './parts'

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
