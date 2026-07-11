# ADR 0012: Accept transitive `quick-xml` advisories RUSTSEC-2026-0194 / RUSTSEC-2026-0195

**Status:** Accepted

## Context

`cargo-deny` reports two **vulnerability** advisories against `quick-xml 0.39.4`:

- **RUSTSEC-2026-0194** — quadratic run time when checking a start tag for duplicate
  attribute names (`BytesStart::attributes()` with the default duplicate check). A
  crafted start tag with N attributes costs O(N²), enabling CPU-exhaustion DoS.
- **RUSTSEC-2026-0195** — unbounded namespace-declaration allocation in `NsReader`
  (`NamespaceResolver::push`), enabling memory-exhaustion (OOM) DoS.

Both are genuine vulnerabilities (not merely unmaintained advisories like ADR 0010/0011).
Their impact, per the advisories themselves, is realized **only when parsing untrusted
XML** — a crafted start tag from an attacker-controlled input stream.

**They are not reachable in this project.** `quick-xml` is **not a direct dependency**.
It has exactly one direct consumer — the **build-time proc-macro** `wayland-scanner` —
reached on a few graph paths that all funnel through it:

```
springmaker → iced 0.14 → iced_winit → winit 0.30 → wayland-client / smithay-client-toolkit
            → wayland-scanner 0.31.10 (proc-macro) → quick-xml 0.39.4
(also: springmaker → rfd 0.17 → wayland-client → wayland-scanner → quick-xml)
```

`wayland-scanner` is a **procedural macro** that runs **at compile time**. It parses the
**trusted, developer-controlled Wayland protocol `.xml` definition files** (shipped with
the wayland crates) to generate Rust bindings. It never parses runtime input, never sees
network or file data supplied by a user of the application, and is not present in the
compiled binary. The DoS conditions (an attacker feeding a crafted start tag to a running
parser) cannot arise: our only `quick-xml` consumer runs once, on trusted input, during
`cargo build`.

**No safe upgrade is available within the current tree.** The remediation for both
advisories is `quick-xml >= 0.41.0`, but `wayland-scanner 0.31.10` pins `quick-xml ^0.39`.
`cargo update -p quick-xml --precise 0.41.0` fails: *"failed to select a version for the
requirement `quick-xml = "^0.39"` … required by wayland-scanner v0.31.10."* Reaching 0.41
requires upgrading the entire `wayland-scanner` / `wayland-client` / `winit` / `iced` 0.14
stack — a major, unrelated dependency migration (iced 0.14 is the current line we target).

## Decision

**Accept** both advisories as **tolerable risk on non-reachability grounds**, linking this
ADR. The vulnerabilities require untrusted-XML parsing; our sole `quick-xml` consumer is a
build-time proc-macro over trusted protocol definitions, so no runtime attack surface
exists. The fix is not reachable without a full GUI-stack upgrade.

The acceptance is made explicit in CI via the `deny.toml` advisory `ignore` list
(`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`), with a comment pointing back to this ADR, so
the gate documents the decision rather than silently passing. The same two IDs are
mirrored in `.cargo/audit.toml` for the separate `cargo-audit` workflow (added 2026-07-10:
that gate reads its own config, not `deny.toml`, and failed on the already-accepted
advisories the first time a `Cargo.*` change path-triggered it after their publication).
Both files must stay in lockstep; the revisit condition below removes the ignores from
**both**. Unlike the `paste` (ADR 0010)
and `ttf-parser` (ADR 0011) acceptances — which are *unmaintained/informational* — this ADR
accepts *vulnerability* advisories, justified by the non-reachability analysis above.

## Consequences

- Two accepted vulnerability advisories remain in the tree, tracked here and
  enforced-as-ignored by `deny.toml`. An *escalation* (a new `quick-xml` advisory with a
  different ID, or a yanked/version change) would surface as a fresh `cargo-deny` failure,
  not silently pass.
- **This acceptance is contingent on `quick-xml` remaining a build-time-only, trusted-input
  dependency.** If the project ever adds a *runtime* `quick-xml` consumer (directly, or via
  a new dependency that parses untrusted XML), this ADR no longer applies and the advisories
  must be re-evaluated as reachable. (springcore/springmaker do not parse XML at all today.)
  This precondition is **enforced mechanically**, not by comment alone: a `[[bans.deny]]`
  entry in `deny.toml` allows `quick-xml` only when wrapped by `wayland-scanner`, so any new
  direct or otherwise-wrapped `quick-xml` dependency fails `cargo-deny` and forces this ADR
  to be revisited — the ID-keyed advisory `ignore` alone would silently keep passing.
- **Revisit when** the `iced` / `winit` / `wayland-scanner` stack is upgraded to a version
  whose `wayland-scanner` permits `quick-xml >= 0.41.0`; at that point remove the two
  ignores from **both** `deny.toml` and `.cargo/audit.toml`, and drop this ADR.

## Alternatives considered

- **Bump `quick-xml` to ≥ 0.41.0.** Not possible — `wayland-scanner 0.31.10` pins
  `quick-xml ^0.39`; `cargo update --precise` fails on that constraint. We cannot patch a
  third-party proc-macro's version requirement.
- **Upgrade the `iced` / `winit` / `wayland` stack.** The "proper" fix, but a large,
  unrelated migration off the iced 0.14 line we currently target; out of scope for the
  change that surfaced this, and potentially not yet available. Tracked as the revisit
  condition above.
- **`RUSTSEC-2026-0195`'s configurable limit.** The fix exposes a
  `set_max_declarations_per_element` knob — but only in `quick-xml >= 0.41.0`, which we
  cannot reach, and only relevant to a runtime `NsReader` consumer we do not have.
- **Fail CI on the advisories.** Rejected — they are non-reachable in our usage (build-time
  proc-macro over trusted input) and unactionable without a major stack upgrade; failing the
  build would block all work on an issue that presents no runtime risk here. This mirrors how
  ADR 0010 / 0011 treat unactionable transitive advisories, with the added non-reachability
  justification that a *vulnerability* (vs unmaintained) acceptance demands.
