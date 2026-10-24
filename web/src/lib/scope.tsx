import { createContext, useContext, useState, type ReactNode } from 'react'

/* Which organisation and project the screens are reading. It is kept per viewer rather than in the
   path, because every screen's url comes from the demo without an organisation in it and a link
   that carries one would be a different app. The cost is that a link to a trace does not say which
   project it was read under; putting both in the path is what fixes that, when it matters. */
type Chosen = { org: string | undefined; project: string | undefined }
type Ctx = Chosen & { chooseOrg: (org: string) => void; chooseProject: (project: string | undefined) => void }

const ScopeContext = createContext<Ctx | null>(null)
const ORG = 'cmt-org'
const PROJECT = 'cmt-project'

function remembered(key: string): string | undefined {
  try {
    return localStorage.getItem(key) ?? undefined
  } catch {
    return undefined
  }
}

function remember(key: string, value: string | undefined) {
  try {
    if (value === undefined) localStorage.removeItem(key)
    else localStorage.setItem(key, value)
  } catch {
    /* A private window refuses storage; the choice then lasts as long as the tab. */
  }
}

export function ScopeProvider({ children }: { children: ReactNode }) {
  const [chosen, setChosen] = useState<Chosen>(() => ({
    org: remembered(ORG),
    project: remembered(PROJECT),
  }))
  const chooseOrg = (org: string) => {
    remember(ORG, org)
    remember(PROJECT, undefined)
    setChosen({ org, project: undefined })
  }
  const chooseProject = (project: string | undefined) => {
    remember(PROJECT, project)
    setChosen((was) => ({ ...was, project }))
  }
  return <ScopeContext.Provider value={{ ...chosen, chooseOrg, chooseProject }}>{children}</ScopeContext.Provider>
}

export function useScope(): Ctx {
  const scope = useContext(ScopeContext)
  if (!scope) throw new Error('useScope outside provider')
  return scope
}
