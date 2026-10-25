import { useState } from 'react'
import { toast } from 'sonner'
import { because } from '@/auth/client'
import { Button } from '@/components/ui/button'
import { NativeSelect } from '@/components/ui/input'
import { aimedAt, useAim, useDestinations, type Routing } from '@/data/integrations'

/* Where an issue is filed is called something different by each provider, and the word matters:
   somebody picking a Linear team should be asked for a team. */
export const PROVIDERS: Record<string, { name: string; place: string }> = {
  linear: { name: 'Linear', place: 'team' },
  github: { name: 'GitHub', place: 'repository' },
  jira: { name: 'Jira', place: 'project' },
}

/* The chooser is one component at two altitudes. Without a project it sets the organisation's
   default; with one it sets that project's own, and "follow the organization" is then a real
   option rather than an absence — which is why the list it offers has an empty entry at the top
   in both cases, saying two different things.

   The list is only asked for once somebody presses Change. Reading it costs a call to the
   provider with the organisation's credential, and a screen that is being read rather than
   changed should not be spending it. */
export function Aim({
  routing,
  project,
  mayChange,
}: {
  routing: Routing
  project?: string
  mayChange: boolean
}) {
  const [choosing, setChoosing] = useState(false)
  const { places, loading, unreachable } = useDestinations(routing.provider, choosing)
  const aim = useAim()
  const what = PROVIDERS[routing.provider] ?? { name: routing.provider, place: 'destination' }
  const { place, following } = aimedAt(routing, project)

  const said = place ? (
    <>
      Issues go to <span className="font-semibold text-foreground">{place.name}</span>
      {project && following ? <span className="text-muted-foreground"> · following the organization</span> : null}
    </>
  ) : project ? (
    <>Nowhere yet: this organization has chosen no default {what.place}.</>
  ) : (
    <>Nowhere yet. Pick the {what.place} an exception should open an issue in.</>
  )

  if (!mayChange) return <span className="text-caption text-muted-foreground">{said}</span>

  if (!choosing) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="flex-1 text-caption text-muted-foreground">{said}</span>
        <Button size="sm" onClick={() => setChoosing(true)}>Change</Button>
      </div>
    )
  }

  if (unreachable) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="flex-1 text-caption text-destructive-text">{because(unreachable, `${what.name} did not say what it offers`)}</span>
        <Button size="sm" onClick={() => setChoosing(false)}>Cancel</Button>
      </div>
    )
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <NativeSelect
        className="h-7 flex-1 min-w-[180px]"
        aria-label={`${what.name} ${what.place} for issues`}
        value={following ? '' : (place?.id ?? '')}
        disabled={loading || aim.isPending}
        autoFocus
        onChange={(e) =>
          aim.mutate(
            { provider: routing.provider, project, destination: e.target.value === '' ? null : e.target.value },
            {
              onSuccess: () => {
                setChoosing(false)
                toast.success('Saved')
              },
              onError: (error) => toast.error(because(error, 'Where issues go did not change')),
            },
          )
        }
      >
        <option value="">
          {project ? `Follow the organization${routing.default ? ` · ${routing.default.name}` : ' · nowhere yet'}` : 'Nowhere'}
        </option>
        {(places ?? []).map((one) => <option key={one.id} value={one.id}>{one.name}</option>)}
      </NativeSelect>
      <Button size="sm" disabled={aim.isPending} onClick={() => setChoosing(false)}>Cancel</Button>
      {loading ? <span className="text-caption text-muted-foreground">Reading what {what.name} offers…</span> : null}
      {/* A provider with nothing on offer is not a provider that failed, and saying so is more
          use than an empty list somebody stares at. */}
      {!loading && places?.length === 0 ? (
        <span className="text-caption text-muted-foreground">{what.name} offers no {what.place} this connection can reach.</span>
      ) : null}
    </div>
  )
}
