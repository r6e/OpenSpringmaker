# Task 1 Report: Workspace scaffolding

## Status: DONE

## Commit

- `1683956` — `chore: scaffold two-crate workspace with CI, licenses, ADRs`

## Files created

| File | Notes |
|------|-------|
| `Cargo.toml` | Workspace manifest, resolver 2, MSRV 1.80, workspace deps |
| `springcore/Cargo.toml` | Engine crate, serde+toml deps, approx+proptest dev-deps |
| `springcore/src/lib.rs` | Empty lib stub with doc comment |
| `springmaker/Cargo.toml` | GUI crate, iced 0.13 + plotters 0.3 + plotters-iced 0.11 |
| `springmaker/src/main.rs` | Stub main() |
| `rustfmt.toml` | edition 2021, max_width 100 |
| `clippy.toml` | too-many-arguments-threshold = 10 |
| `.gitignore` | /target, *.rs.bk, Cargo.lock.orig |
| `LICENSE-MIT` | Canonical MIT, year 2026, "OpenSpringmaker contributors" |
| `LICENSE-APACHE` | Canonical Apache-2.0 full text with APPENDIX |
| `README.md` | Description, build/run/test instructions, dual-license note |
| `ARCHITECTURE.md` | Two-crate split, SI units, scenario solver, materials, ADR index |
| `docs/adr/0001-scenario-based-solver.md` | Four scenarios + numeric kernel vs. generic solver |
| `docs/adr/0002-si-internal-newtype-units.md` | Newtype quantities vs. uom |
| `docs/adr/0003-unit-native-mts-coefficients.md` | Shigley Table 10-4 unit-dependence rationale |
| `docs/adr/0004-two-crate-workspace.md` | Engine/GUI separation |
| `docs/adr/0005-absolute-stability-buckling.md` | Shigley Eq. 10-10; Eq. 10-11 deferred |
| `docs/adr/0006-allowable-stress-simplification.md` | Scalar allowable-pct vs. full SMI curves |

## Files modified

| File | Change |
|------|--------|
| `CONTRIBUTING.md` | Added "## License" section with dual MIT/Apache-2.0 statement and no-commercial-product rule; all existing content retained |

## Files deleted

| File | Reason |
|------|--------|
| `LICENSE` | Replaced by `LICENSE-MIT` + `LICENSE-APACHE` per dual-license plan |

## Reconciliation decisions

**License:** Deleted the single `LICENSE` (MIT, "Open Springmaker Contributors"). Created
`LICENSE-MIT` with the brief's holder string "OpenSpringmaker contributors" (lowercase c,
no space in name) and `LICENSE-APACHE` with full canonical text including the APPENDIX.

**`CONTRIBUTING.md`:** Existing file already covered all plan requirements (TDD workflow,
fmt/clippy gate, conventional commits, citation requirement, no-commercial-product rule).
Only gap was a dual-license section — added that at the bottom before "## Reporting issues".
No content was removed.

**`ci.yml`:** Retained unchanged. The existing workflow already runs `cargo fmt --all --
--check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace --all-features`, and `cargo build --workspace --release`, across
ubuntu/macos/windows with an MSRV (1.80) check job. This is a superset of the plan's
four gates. Replacing it with the minimal plan YAML would be a downgrade.

**`.gitignore`, `rustfmt.toml`, `clippy.toml`:** All absent before; created per brief.

**ADR count:** Brief step header says "five ADRs" but the Files list and bullet list both
enumerate six (0001–0006). Created all six; "five" is a stale typo in the step header.

**No commercial-product references:** Scanned all created/modified files. No product or
vendor names introduced.

## Verification output

```
$ cargo build --workspace
   Locking 473 packages
   Compiling ... (473 crates)
   Compiling springcore v0.1.0
   Compiling springmaker v0.1.0
    Finished `dev` profile in 29.59s

$ cargo fmt --all -- --check
(no output — clean)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile in 14.92s
(no warnings)

$ cargo test --workspace
test result: ok. 0 passed; 0 failed; 0 ignored
```

## Concerns

1. **MSRV vs. iced MSRV:** The workspace sets `rust-version = "1.80"` and CI has an
   MSRV-1.80 `cargo check` job. Local build used current stable. If `iced 0.13` or its
   transitive deps require a Rust version above 1.80, the MSRV CI job will fail. This is
   a CI-only risk and was not detectable in local verification.

2. **`deny.toml` absent:** A `deny.yml` workflow exists in `.github/workflows/` but
   `deny.toml` is not present in the repo. The `cargo-deny` check will fail in CI until
   `deny.toml` is created. This is a pre-existing gap, out of scope for Task 1, but
   worth noting.

3. **`Cargo.lock` committed:** Lock file is committed (standard for binary crates in a
   workspace with a binary). The locked versions are: `iced 0.13.1`, `plotters 0.3.7`,
   `plotters-iced 0.11.0`, `toml 0.8.23`. All resolved without conflict.

## Review fix round 1

**Commit:** `a0363df` — `fix: replace non-canonical LICENSE-APACHE APPENDIX and pin ADR 0005 citation`

### Fix 1 (Critical): LICENSE-APACHE canonical text

The original `LICENSE-APACHE` APPENDIX contained a fabricated paragraph instructing
users to include a `"SUPPLEMENTARY LICENSE INFORMATION"` header — a phrase that does not
appear anywhere in the authoritative Apache-2.0 text at
`https://www.apache.org/licenses/LICENSE-2.0.txt`.

The canonical APPENDIX boilerplate reads:

> "We also recommend that a file or class name and description of purpose be included on
> the same 'printed page' as the copyright notice for easier identification within
> third-party archives."

The file was overwritten with the full canonical text fetched from apache.org. The
canonical notice template (without the fabricated SUPPLEMENTARY paragraph) follows.

**Sanity check results:**

- `"SUPPLEMENTARY LICENSE INFORMATION"` present: **0 occurrences (PASS)**
- Line 1: `Apache License` ✓
- Line 2: `Version 2.0, January 2004` ✓
- Nine numbered sections (1–9) ✓
- `END OF TERMS AND CONDITIONS` at line 172 ✓
- `APPENDIX: How to apply the Apache License to your work.` at line 174 ✓

### Fix 2 (Minor): ADR 0005 edition-specific citation

The vague citation `"cited from Shigley's Mechanical Engineering Design, Chapter 10"`
was replaced with an auditable reference:

> "taken from Shigley's Mechanical Engineering Design, 10th ed., Eq. 10-10
> (absolute-stability criterion); the α end-condition constant per Table 10-2."

The existing note that the deflection-ratio curve (Eq. 10-11) is deferred was
preserved. No values were invented; the 10th edition edition/equation/table numbers
pin the existing `α_cr ≈ 2.63` claim.

### Verification

```
$ cargo build --workspace
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.45s

$ cargo fmt --all -- --check
(no output — clean)
```

Both commands unaffected by the docs/license-only changes, as expected.
