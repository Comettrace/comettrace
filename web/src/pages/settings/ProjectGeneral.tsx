import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input, NativeSelect } from '@/components/ui/input'
import { useTeams } from '@/data/organization'
import { ConfirmDelete, Danger, H, Sub, saved } from './parts'

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
