import { Plus } from 'lucide-react'
import { useState } from 'react'
import { Link } from '@/lib/router'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, Input } from '@/components/ui/input'
import { Table, Td, Th, Tr } from '@/components/ui/misc'
import { useProjects } from '@/data/organization'
import { H } from './parts'

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
