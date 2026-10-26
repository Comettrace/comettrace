# 0017. Provisioning belongs behind a port

Status: accepted
Date: 2026-10-26

## Context
The command line provisions a deployment: `bootstrap`, `create-account`,
`reset-password`, `create-org`, `create-project`, `grant-role`, `issue-key`,
`revoke-key`. Every one of them was written in `apps/server/src/main.rs`, and
every one reached for a concrete adapter rather than a port:
`PostgresMemberships::founded`, `::found` and `::grant`,
`PostgresProjects::only` and `::claim`, `PostgresDirectory::issue` and
`::revoke`, `PostgresResets::account_for` and `PostgresAccounts::create`.

Those methods are inherent to the adapters. `Memberships`
(`crates/core/src/membership.rs`) declares only `of`, and `Projects`
(`crates/core/src/catalogue.rs`) declares `of`, `create`, `rename`, `forget`
and `assign`. Nothing in the domain named `founded`, `found`, `grant`, `only`,
`claim`, `issue` or `revoke`, so the command line could not be redirected
through a port at all: a second adapter for any of those would have left the
CLI broken, and the rules it encodes had no home in the domain.

The rules are real, and they are not printing:

- an organisation is founded at most once per deployment, and a second
  `bootstrap` changes nothing
- an account is created locked, then invited, and an address that already
  carries an account is invited rather than recreated
- a fresh organisation holds one project named `default`
- a key for an organisation holding exactly one project takes that project;
  with none or several it names none, and its spans match no project filter
- an invitation lasts 24 hours and can be spent once

Each was enforced once, in a binary, with no test. The HTTP side has the
opposite shape: `Catalogue`, `OrgSettings`, `Roster` and `Whoami` are domain
services over ports, and every rule they carry is unit-tested.

## Decision
Provisioning is a domain service over one driven port, like every other use
case. `apps/server` keeps the clap definitions, the wiring and the printing,
and nothing else.

`crates/core/src/provisioning.rs` holds:

```rust
pub enum ProvisionError {
    NoSuchAccount,
    NoSuchOrg,
    Taken,
    Unreachable,
}

pub trait ControlPlane {
    fn founded(&self) -> impl Future<Output = Result<Option<TenantId>, ProvisionError>> + Send;
    fn found(&self, name: &OrgName, owner: &Email) -> ...TenantId;
    fn grant(&self, org: TenantId, member: &Email, role: OrgRole) -> ...();
    fn account_for(&self, email: &Email) -> ...Option<AccountId>;
    fn create_locked_account(&self, email: &Email) -> ...AccountId;
    fn invite(&self, account: AccountId, digest: SecretHash) -> ...();
    fn claim_project(&self, org: TenantId, name: &ProjectName) -> ...ProjectId;
    fn only_project(&self, org: TenantId) -> ...Option<ProjectId>;
    fn issue_key(&self, org: TenantId, project: Option<ProjectId>) -> ...IngestKey;
    fn revoke_key(&self, prefix: &str) -> ...bool;
}

pub struct Provisioning<C> { control_plane: C }
```

`Provisioning<C>` carries the eight workflows and returns what the caller
prints, never printing itself: `Bootstrapped::{AlreadyFounded, Founded}`,
`Founded { org, project, project_name, invitation }` and
`Issued { key, project }`. A deployment that already holds an organisation is
an outcome, not an error, because that is what makes `bootstrap` idempotent
rather than merely safe to retry.

The `&str` arguments the adapters take become `OrgName` and `Email` at the
port, parsed by the CLI before the call. An address that is not an address is
now refused by `create-org` and `grant-role` before any SQL runs.

`crates/storage/src/provisioning.rs` implements the port once, as
`PostgresControlPlane`, by delegating to the adapters that already hold the
SQL and mapping their errors: `MembershipError::NoSuchAccount` and
`ProjectError::{Taken, NoSuchOrg}` keep their meaning, everything else is
logged and becomes `Unreachable`. It owns the 24-hour `INVITE_WINDOW` and the
Argon2 hasher that locks a new account, so neither leaks into the domain.

## Alternatives rejected
- Add the operations to the existing `Memberships`, `Projects`, `Accounts` and
  `PasswordHasher` traits, as this ADR first proposed. It would change four
  traits that already have implementations and fakes across three crates, so
  every one of those would have to grow methods it has no use for. One port
  for one use case costs less and says more.
- Put the port traits in `crates/storage` beside their implementations: the
  dependency would point outward, and the domain could not hold the rules.
- Leave the workflows in `main.rs` and test the binary end to end: the
  self-hosting CI job already does that, and it takes a running stack, a
  Postgres, a ClickHouse and a Redis to assert that `bootstrap` is idempotent.
  A unit test over a fake does it in microseconds.

## Consequences
- Gains: every provisioning rule is now a unit test over an in-memory control
  plane, 16 of them in `crates/core/src/provisioning.rs`. The CLI names no
  concrete adapter, so `apps/server` matches the dependency direction the rest
  of the workspace keeps.
- Costs: one ten-method port. It is wide because provisioning touches four
  tables, and it is cohesive because one operator-driven use case needs all of
  it.
- What becomes harder: nothing in the HTTP path, which does not use this port.
  The adapters keep their inherent methods; `PostgresControlPlane` is their
  only caller outside the crate's own tests.
