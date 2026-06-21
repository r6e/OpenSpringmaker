# ADR 0007: Accept transitive `lru` advisory RUSTSEC-2026-0002

**Status:** Accepted

## Context

GitHub Dependabot raised a **low**-severity alert (GHSA-rhfx-m35p-ff5j /
RUSTSEC-2026-0002, CVSS 0, no CVE) against the `lru` crate:

> `IterMut` violates Stacked Borrows by invalidating an internal pointer.

This is an **unsound** advisory: undefined behavior under the Stacked Borrows
aliasing model (observable under Miri), not a remotely exploitable
vulnerability. Vulnerable range `>= 0.9.0, < 0.16.3`; fixed in `lru 0.16.3`.

`lru` is **not a direct dependency**. It is pulled in transitively by the GUI
crate's text-rendering stack:

```
springmaker → iced 0.13.1 → iced_renderer → iced_wgpu 0.13.5
            → iced_glyphon 0.6.0 → lru 0.12.5
```

`iced_glyphon 0.6.0` pins `lru = "^0.12.1"`, and the highest `0.12.x` release is
`0.12.5` (our locked version) — the fix landed only in `lru 0.16.3`, after
breaking `0.13`/`0.14`/`0.15`/`0.16` releases. So the patched `lru` is
**unreachable within the `iced 0.13` line**: `cargo update -p lru --precise
0.16.3` fails version selection, and no `cargo update` of `iced`/`iced_wgpu`/
`iced_glyphon` moves it. (The `iced 0.14` line *does* fix it — see Alternatives.)
The crate's own security gate already treats this as informational —
`cargo-audit` (rustsec/audit-check) passes, reporting unsound advisories as
warnings rather than failures.

## Decision

**Accept** the advisory for now and **dismiss** the Dependabot alert as
**tolerable risk** — a fix exists upstream (`iced 0.14`) but is not reachable via
`cargo update` and is blocked from a manual bump by `plotters-iced 0.11` (see
Alternatives) — linking this ADR.

Alternatives considered and rejected:

- **`cargo update` to a patched `lru`** — impossible; `iced_glyphon 0.6.0`
  constrains `lru` to `^0.12`.
- **`[patch.crates-io]` override to `lru 0.16.x`** — `0.12 → 0.16` is a
  breaking API change; `iced_glyphon` would not compile against it.
- **Bump `iced` to 0.14** — `iced 0.14.0` *does* resolve the advisory: it
  replaced `iced_glyphon` with `cryoglyph 0.1`, which depends on `lru ^0.16`
  (resolving to `lru 0.16.4`). It is not adoptable here, though: `0.13 → 0.14` is
  a breaking major bump, and — the hard blocker — `plotters-iced 0.11.0` (the
  latest release, which this crate uses for the results chart) still requires
  `iced ^0.13` and is incompatible with `iced 0.14`. Adopting 0.14 would require
  `plotters-iced` to ship 0.14 support, or replacing it. No `iced 0.13.x` point
  release moves the text stack off the vulnerable `lru`.
- **Add a `deny.toml` to ignore the advisory** — the `cargo-deny` CI job runs
  only when a `deny.toml` exists (none today), so introducing one solely to
  silence this advisory would activate `cargo-deny`'s full `check all` suite
  (licenses, bans, duplicate crates) and surface unrelated failures. Out of
  scope for this change.

## Consequences

- No code or dependency change ships for this advisory; the dependency tree is
  unchanged.
- Risk is low and bounded: the unsoundness lives in `lru::IterMut`, exercised
  only inside `iced_glyphon`'s internal glyph cache, never called directly by
  this project. It is a strict-aliasing soundness defect, not an exploitable
  vulnerability, in a desktop GUI dependency.
- **Revisit when** `plotters-iced` ships support for `iced 0.14` (or is replaced
  by a charting approach that does), unblocking the `iced 0.13 → 0.14` upgrade —
  `iced 0.14` already resolves the advisory (`cryoglyph → lru 0.16`). At that
  point, bump `iced`, drop this ADR, and let the upgrade clear the advisory. The
  daily scheduled `cargo-audit` run and Dependabot will keep surfacing it if its
  status escalates.
