import { ShieldCheck } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter } from '@/components/ui/dialog'
import { Field, Input } from '@/components/ui/input'
import { Table, Td, Th, Tr } from '@/components/ui/misc'
import { H, Row } from './parts'

export function Security() {
  const [twofa, setTwofa] = useState(false)
  const [setup, setSetup] = useState(false)
  const [code, setCode] = useState('')
  return (
    <>
      <H title="Login & security" sub="How you sign in, and where you are signed in." />
      <Card className="overflow-hidden">
        <Row title="Sign-in method" sub="Email and password. Single sign-on replaces it when the organisation turns it on."><Badge variant="outline">Email and password</Badge></Row>
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
