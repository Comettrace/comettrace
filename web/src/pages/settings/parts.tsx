import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, Input } from '@/components/ui/input'

/* The furniture every settings screen is built out of: a heading, a labelled row, a danger card
   and the dialog that makes you type a name before deleting something. It lives here rather than
   in components/ui because none of it has been wanted anywhere else — the day one of these is
   asked for by a screen outside this folder is the day it belongs there instead. */

export function H({ title, sub, children }: { title: string; sub?: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="flex flex-1 flex-col gap-1"><h2 className="text-title">{title}</h2>{sub ? <span className="text-control text-muted-foreground">{sub}</span> : null}</div>
      {children}
    </div>
  )
}

export const Sub = ({ children }: { children: React.ReactNode }) => <h3 className="mt-2 text-[12px] font-semibold uppercase tracking-[.05em] text-muted-foreground">{children}</h3>

export function Row({ title, sub, children }: { title: string; sub?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-border-subtle px-4 py-3 last:border-b-0">
      <span className="flex flex-[1_1_260px] flex-col"><span className="text-control font-semibold">{title}</span>{sub ? <span className="text-caption text-muted-foreground">{sub}</span> : null}</span>
      {children}
    </div>
  )
}

export const saved = () => toast.success('Saved')

export function Danger({ title, text, action, onClick }: { title: string; text: string; action: string; onClick: () => void }) {
  return (
    <Card className="flex flex-wrap items-center gap-4 border-destructive-border p-4">
      <div className="flex flex-[1_1_320px] flex-col gap-0.5"><span className="font-semibold">{title}</span><span className="text-control text-muted-foreground">{text}</span></div>
      <Button size="sm" className="text-destructive-text" onClick={onClick}>{action}</Button>
    </Card>
  )
}

/* Without `onConfirm` nothing deletes this kind of thing yet, and the dialog says so rather than
   pretending: it is the one way a screen can offer the button honestly before the route exists. */
export function ConfirmDelete({ open, onOpenChange, name, what, body, onConfirm, busy }: { open: boolean; onOpenChange: (o: boolean) => void; name: string; what: string; body: string; onConfirm?: () => void; busy?: boolean }) {
  const [v, setV] = useState('')
  return (
    <Dialog open={open} onOpenChange={(o) => { onOpenChange(o); setV('') }}>
      <DialogContent role="alertdialog" title={`Delete ${name}?`} description={body}>
        <Field label={<>Type <span className="font-mono">{name}</span> to confirm</>} htmlFor="cd-in"><Input id="cd-in" mono value={v} onChange={(e) => setV(e.target.value)} autoFocus /></Field>
        <DialogFooter><Button onClick={() => onOpenChange(false)}>Cancel</Button><Button variant="destructive" disabled={v !== name || busy} onClick={() => { if (onConfirm) { onConfirm(); return } onOpenChange(false); toast('Nothing was deleted', { description: `No route deletes a ${what} yet, so ${name} stays in place.` }) }}>Delete {what}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
