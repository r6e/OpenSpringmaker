# ADR 0011: Accept transitive `ttf-parser` advisory RUSTSEC-2026-0192

**Status:** Accepted

## Context

`cargo-deny` reports an **unmaintained** advisory (RUSTSEC-2026-0192) against the
`ttf-parser` crate:

> The author of `ttf-parser` has stated that the crate is unmaintained and will
> not receive further fixes.

This is an **informational** advisory — `ttf-parser` is unmaintained, not
vulnerable. There is no CVE and no behavioral defect; the code works, and the
advisory itself states **"No safe upgrade is available!"** (`0.25.1` is the
latest published release).

`ttf-parser` is **not a direct dependency**. It rides in transitively through the
GUI font/text/chart stack, on three independent paths:

```
springmaker -> iced 0.14 -> ... -> cosmic-text 0.15 -> fontdb -> ttf-parser                     (text shaping)
springmaker -> plotters 0.3 -> ab_glyph -> owned_ttf_parser -> ttf-parser                       (chart glyphs)
springmaker -> iced 0.14 -> iced_winit -> winit -> sctk-adwaita -> ab_glyph -> ... -> ttf-parser (window decorations)
```

`cosmic-text` (iced's text engine), `plotters` (our charting crate), and
`sctk-adwaita` (winit's client-side decorations) all use `ttf-parser` internally.
We do not call it directly. We do enable `plotters`'s `ab_glyph` feature (ADR 0009,
to bundle a chart font), which is what pulls `ttf-parser` on the `plotters` path —
but the `iced` -> `cosmic-text` -> `fontdb` path is non-optional and pulls
`ttf-parser` regardless, so no feature toggle of ours removes it.

The advisory's suggested alternative — `skrifa`, from the Google Fonts
`fontations` project — targets **direct** users who can swap their own parsing.
It is irrelevant here: we have no `ttf-parser` usage to replace, and we cannot
rewrite `cosmic-text` / `plotters` / `sctk-adwaita`. No `cargo update` removes
`ttf-parser` within the `iced 0.14` / `plotters 0.3` line, and the advisory
confirms no safe upgrade exists.

## Decision

**Accept** the advisory as **tolerable risk**, linking this ADR. There is no
action available: `ttf-parser` is an unmaintained-but-functional, transitive
dependency of the GUI text engine, the charting crate, and the window
decorations, with no reachable replacement.

The acceptance is made explicit in CI via the `deny.toml` advisory `ignore` list
(`RUSTSEC-2026-0192`), with a comment pointing back to this ADR, so the gate
documents the decision rather than silently passing — mirroring the `paste`
acceptance (ADR 0010), the only other entry currently in the ignore list and the
same advisory class (unmaintained).

## Consequences

- One more accepted informational advisory remains in the tree, tracked here and
  enforced-as-ignored by `deny.toml`. An *escalation* — e.g. a vulnerability filed
  against `ttf-parser` — would surface as a new advisory ID rather than silently
  pass.
- **Revisit when** `cosmic-text` / `fontdb`, `plotters` / `ab_glyph`, or
  `sctk-adwaita` migrate off `ttf-parser` (e.g. to `skrifa` / `fontations`), or
  when an `iced` / `plotters` upgrade changes the font stack. At that point remove
  the `deny.toml` ignore and drop this ADR.

## Alternatives considered

- **Replace `ttf-parser`.** Not possible — we have no direct dependency on it; it
  is internal to `cosmic-text`, `plotters`, and `sctk-adwaita`. We cannot patch a
  third-party crate's parser choice.
- **Migrate to `skrifa`.** Not actionable for us — the migration must happen in
  the upstream crates (`fontdb`, `ab_glyph`). The `fontations` crates already
  appear in our tree (pulled by `cosmic-text`), but the `ttf-parser` paths remain
  until upstream drops them.
- **Downgrade / pin `ttf-parser`.** No earlier version is maintained either, and
  the advisory states no safe upgrade exists.
- **Fail CI on the advisory.** Rejected — it is informational (unmaintained, not a
  vulnerability) and unactionable; failing the build would block all work on an
  issue we cannot fix, contrary to how ADR 0010 already treats the same advisory
  class (the `paste` acceptance — the sole same-class precedent). (ADR 0007's `lru`
  advisory was a *different* class — an unsound defect *with* a fix — since resolved
  by the iced 0.13 -> 0.14 upgrade and no longer in the ignore list; it is not a
  same-class precedent, only an example of the "revisit when upstream moves"
  pattern.)
