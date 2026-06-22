# ADR 0010: Accept transitive `paste` advisory RUSTSEC-2024-0436

**Status:** Accepted

## Context

`cargo-audit` / Dependabot report an **unmaintained** advisory
(RUSTSEC-2024-0436, dated 2024-10-07) against the `paste` crate:

> The creator of `paste` has stated the project is no longer maintained and
> archived the repository.

This is an **informational** advisory — `paste` is archived, not vulnerable.
There is no CVE and no behavioral defect; the code works and is widely depended
upon across the ecosystem.

`paste` is **not a direct dependency**. It is a `proc-macro` pulled in
transitively, and only on the macOS/Apple graphics path:

```
springmaker → iced 0.14 → iced_renderer → iced_wgpu → wgpu → wgpu-hal
            → metal 0.32 → paste 1.0.15
```

`metal` (Apple's Metal backend for `wgpu`) uses `paste` internally. We do not
call `paste`, and nothing in our code or the parts of `wgpu` we control selects
it — it rides in with the GPU backend `iced` requires.

The advisory's suggested alternatives (`pastey`, `with_builtin_macros`) target
**direct** users who can swap their own `paste!` calls. They are irrelevant
here: we have no `paste` usage to replace, and we cannot rewrite `metal`/`wgpu`.
No `cargo update` removes `paste` within the `iced 0.14` / `wgpu 27` line.

The project's security gate already treats this correctly — the `cargo-audit`
job (the Security Audit workflow) reports unmaintained advisories as
**warnings**, not failures, so CI passes.

## Decision

**Accept** the advisory and **dismiss** the Dependabot alert as **tolerable
risk**, linking this ADR. There is no action available: `paste` is an
archived-but-functional, transitive, proc-macro dependency of the Apple GPU
backend, with no reachable replacement.

The acceptance will be made explicit in CI via the `deny.toml` advisory
`ignore` list (`RUSTSEC-2024-0436`), with a comment pointing back to this ADR,
so the gate documents the decision rather than silently passing. (This reverses
ADR 0007's earlier deferral of a `deny.toml` — it is now being added
deliberately; the follow-up that introduces it scopes `cargo-deny`'s full
`check all` suite, licenses and bans included, against the now-clean tree.)

## Consequences

- One accepted informational advisory remains in the tree, tracked here and (once
  added) enforced-as-ignored by `deny.toml`. `cargo-audit` continues to surface
  it as a warning, so an *escalation* (e.g. a vulnerability filed against `paste`)
  would still show up.
- **Revisit when** `wgpu`/`metal` drop or replace `paste` (e.g. with `pastey`),
  or when an `iced`/`wgpu` upgrade changes the backend dependency. At that point
  remove the `deny.toml` ignore and drop this ADR.

## Alternatives considered

- **Replace `paste`.** Not possible — we have no direct dependency on it; it is
  internal to `metal`. We cannot patch a third-party crate's macro choice.
- **Drop the Metal backend / `wgpu`.** `iced 0.14` requires `wgpu` for
  rendering, and `metal` is its macOS backend. Not an option.
- **Fail CI on the advisory.** Rejected — it is informational (unmaintained,
  not a vulnerability) and unactionable; failing the build would block all work
  on an issue we cannot fix, contrary to the gate's existing warn-not-fail policy
  for unmaintained advisories.
