# ADR 0013: Public-enum exhaustiveness policy

**Status:** Accepted

## Context

springcore's public enums were inconsistent: compression's
`BindingConstraint` deliberately omits `#[non_exhaustive]` (a PR #32 scope
decision) so that adding a variant is a compile error at every downstream
`match`, while extension's `ExtBindingConstraint` and torsion's
`TorBindingConstraint` both carried the attribute — forcing the GUI to hold a
silent `_ => "other"` wildcard arm in each family that would hide any future
binding limit at runtime instead of surfacing it at compile time. A review
panel on the extension work surfaced the torsion twin.

`FrictionModel` omits `#[non_exhaustive]` for compile-surface exposure (PR
#32): it is constructed and variant-referenced by the GUI but not
exhaustively label-matched there — the loudness is deliberate so that adding
a friction-model variant forces every call site to be audited.

## Decision

Enums that a downstream layer must exhaustively `match` on (binding
constraints) carry NO `#[non_exhaustive]`: a new variant is a deliberate,
loud compile-time break at every match site, and the wildcard arm is
forbidden.

Enums whose `#[non_exhaustive]` removal is a deliberate compile-surface
commitment (e.g. `FrictionModel` per PR #32) also carry no attribute, but
for the distinct reason that any new variant must force an audit of every
construction and reference site — not because the GUI exhaustively matches
them.

Enums that downstream code only displays or iterates (e.g. `CycleLife`,
`DiaPolicy`, `HookSpec` — constructed or shown via `Display` + an `ALL_*`
const, never matched in the GUI) keep `#[non_exhaustive]`; extending them is
additive and silent by design.

`ExtBindingConstraint` and `TorBindingConstraint` both lose the attribute and
their GUI wildcard arms die.

## Consequences

- Adding a binding-constraint variant now fails the workspace build until
  every match site handles it — the failure mode we want.
- Removing `#[non_exhaustive]` is technically a semver-major change;
  springcore is workspace-internal and unpublished, so this is free today.
  If springcore is ever published, match-surface enums are a deliberate
  major-version commitment.
- Alternative considered: keep the attribute and render a visible
  "unknown constraint" label from the wildcard. Rejected — it converts a
  compile-time signal into a runtime discovery.
