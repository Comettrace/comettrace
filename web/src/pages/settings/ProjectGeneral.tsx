import { useState } from 'react'
import { toast } from 'sonner'
import { because } from '@/auth/client'
import { useOrganizations } from '@/auth/session'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Field, Input, NativeSelect } from '@/components/ui/input'
import { useFilings } from '@/data/integrations'
import {
  projectNameProblem,
  useAssignProject,
  useCatalogue,
  useForgetProject,
  useRenameProject,
} from '@/data/projects'
import { useTeams } from '@/data/teams'
import { Link } from '@/lib/router'
import { useScope } from '@/lib/scope'
import { Aim, PROVIDERS } from './aim'
import { ConfirmDelete, Danger, H, Row, Sub } from './parts'

export function ProjectGeneral() {
  const { active } = useOrganizations()
  const { projects, nothing } = useCatalogue()
  const { teams } = useTeams()
  const { routings } = useFilings()
  const { project: chosen, chooseProject } = useScope()
  const rename = useRenameProject()
  const assign = useAssignProject()
  const forget = useForgetProject()
  /* Null means the field is still showing what the server holds, so a rename made elsewhere
     reaches it; anything else is what is being typed. */
  const [typed, setTyped] = useState<string | null>(null)
  const [del, setDel] = useState(false)

  const catalogue = projects ?? []
  /* The settings navigation chooses a project, and so does the switcher at the top of every
     screen; they are the same choice. "All projects" is not one, so the first stands in. */
  const project = catalogue.find((one) => one.id === chosen) ?? catalogue[0]
  const mayChange = active?.role === 'owner' || active?.role === 'admin'

  if (!project) {
    return (
      <>
        <H title="General" sub="The project this organization holds telemetry under." />
        <Card className="px-4 py-3 text-control text-muted-foreground">
          {projects ? <>This organization holds no project yet. <Link to="/settings/projects">Make one</Link> and come back.</> : nothing}
        </Card>
      </>
    )
  }

  const name = typed ?? project.name
  const problem = typed === null ? null : projectNameProblem(typed)
  const unchanged = name.trim().toLowerCase() === project.name

  return (
    <>
      <H title="General" sub={`Project ${project.name}.`} />
      <Card className="flex flex-col gap-3.5 p-4">
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(260px,100%),1fr))] gap-3">
          <Field label="Project name" htmlFor="pg-n" hint="Renaming keeps keys and signals: nothing anywhere holds the name.">
            <div className="flex gap-2">
              <Input id="pg-n" mono value={name} disabled={!mayChange} aria-invalid={problem ? true : undefined} onChange={(e) => setTyped(e.target.value)} />
              <Button size="sm" className="h-8" variant="primary" disabled={!mayChange || unchanged || Boolean(problem) || rename.isPending} onClick={() => rename.mutate({ project: project.id, name }, { onSuccess: () => { setTyped(null); toast.success('Saved') }, onError: (error) => toast.error(because(error, 'The project was not renamed')) })}>Save</Button>
            </div>
          </Field>
          <Field label="Owner team" hint={mayChange ? 'A project has one owner team or none.' : 'Only an owner or an administrator can change this.'}>
            <NativeSelect aria-label="Owner team" value={project.team?.id ?? ''} disabled={!mayChange || assign.isPending} onChange={(e) => assign.mutate({ project: project.id, team: e.target.value === '' ? null : e.target.value }, { onError: (error) => toast.error(because(error, 'The project did not change hands')) })}>
              <option value="">No team</option>
              {(teams ?? []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </NativeSelect>
          </Field>
        </div>
        {problem ? <span className="text-caption text-destructive-text">{problem}</span> : null}
        <span className="text-control"><span className="text-muted-foreground">Project id</span> <span className="font-mono">{project.id}</span></span>
        <span className="text-control"><span className="text-muted-foreground">Api keys that still authenticate</span> <span className="font-mono">{project.keys}</span></span>
      </Card>
      {/* Where this project's issues go. A project that has chosen nothing follows the
          organisation, which the chooser offers as a first option rather than leaving blank: two
          different nothings would otherwise look like one. A provider nobody is connected to has
          no row here at all. */}
      {(routings ?? []).length > 0 ? (
        <>
          <Sub>Where issues go</Sub>
          <Card className="p-0">
            {(routings ?? []).map((routing) => (
              <Row
                key={routing.provider}
                title={PROVIDERS[routing.provider]?.name ?? routing.provider}
                sub={`The ${PROVIDERS[routing.provider]?.place ?? 'destination'} an exception from ${project.name} opens an issue in.`}
              >
                <span className="flex flex-[2_1_320px] flex-col"><Aim routing={routing} project={project.id} mayChange={mayChange} /></span>
              </Row>
            ))}
          </Card>
        </>
      ) : null}
      <p className="m-0 text-caption text-muted-foreground">How long a signal is kept is a property of the deployment rather than of one project: it is <span className="font-mono">spans.retention_days</span>, and the span store applies it on its own schedule.</p>
      {mayChange ? (
        <>
          <Danger title={`Delete ${project.name}`} text="Its api keys stop authenticating at once. Spans already written wait out the retention the deployment set." action="Delete project" onClick={() => setDel(true)} />
          <ConfirmDelete
            open={del}
            onOpenChange={setDel}
            name={project.name}
            what="project"
            body={`Its ${project.keys} api key${project.keys === 1 ? '' : 's'} go with it. Spans already written stay in the span store until their retention expires, unreachable from here.`}
            busy={forget.isPending}
            onConfirm={() =>
              forget.mutate(project.id, {
                onSuccess: () => {
                  setDel(false)
                  chooseProject(undefined)
                  toast(`Project ${project.name} deleted`)
                },
                onError: (error) => toast.error(because(error, 'The project was not deleted')),
              })
            }
          />
        </>
      ) : null}
    </>
  )
}
