# OpenSpringmaker — Editable Materials Database

- **Status:** Approved (design), pending implementation plan
- **Date:** 2026-06-21
- **Sub-project:** 2 of the roadmap (builds on sub-project 1, merged to `main`)

## 1. Purpose & context

Sub-project 1 shipped an immutable `MaterialSet`: four curated spring materials
loaded once from a bundled TOML file, exposed read-only to the calculator. This
sub-project turns materials into an **editable, persisted database**: a curated
read-only set bundled with the app, plus a user-editable overlay; a comprehensive
(~20+) cited material set; an additional strength-equation form and an
(informational) temperature field; and a GUI editor for managing user materials.

Non-negotiable project constraints carry over: **no references to any commercial
product or vendor** in persisted files; **every formula and material constant
cited inline**; **strict TDD**; **professional-grade accuracy**; **modern UI/UX**;
SI canonical internally; dual MIT/Apache.

## 2. Scope

### In scope
- A mutable **`MaterialStore`** in `springcore` = curated (read-only) ∪ user
  (editable overlay), with add / clone / edit / remove of user materials,
  validation, and load/save of the user overlay.
- **`MtsForm::Rational`** — the 5-parameter rational strength form — added
  alongside the existing Constant / PowerLaw / Polynomial forms.
- An optional, **informational** `max_service_temperature` field on materials.
- Overlay **persistence** to a platform config dir (TOML, schema-versioned,
  atomic writes, graceful failure).
- Conversion of the deferred-panic `From<RawMaterial>` into fallible parsing.
- Expansion of the bundled curated set to **~20+ cited materials**, governed by a
  **data-correctness contract** (§7).
- A **GUI materials editor** in `springmaker` (list + add/clone/edit/remove + save),
  with the calculator picker reading the merged store.

### Out of scope (later cycles)
- Using temperature in calculations (derating, modulus-vs-temperature curves):
  the field is data-only this cycle.
- Customers / projects / part-number design database, import/export of designs.
- Any spring family beyond what sub-project 1 covers.

## 3. Architecture & module layout

`springcore` (engine, no GUI deps):
- `material.rs` — existing `Material`, `MtsEquation`, `MtsForm`, `StrengthUnits`,
  `Endurance` types, extended with `Rational` and `max_service_temperature`.
- `material_store.rs` (new) — `MaterialStore`: the merged curated+user collection
  and all mutation/query logic.
- `material_persist.rs` (new) — load/save of the user overlay TOML (config-dir
  resolution, atomic write, schema version, error handling).

(Implemented as flat `material_store.rs` / `material_persist.rs` siblings of
`material.rs` rather than a `material/` module directory; each file stays focused.)

`springmaker` (GUI):
- `materials_view.rs` (or a section of the view layer) — the editor screen.
- `materials_form.rs` — pure, testable form-to-material logic (no iced), mirroring
  the `form.rs` pattern from sub-project 1.

## 4. Data model

### MtsForm::Rational
`Sut = (P0·d^P4 + P1) / (P2·d^P4 + P3)`, coefficients `[P0, P1, P2, P3, P4]`,
evaluated in the material's native units (like the other forms). **Denominator
guard:** if `P2·d^P4 + P3 == 0` (or non-finite result), `evaluate` returns
`SpringError::InconsistentInputs` — never inf/NaN (same crash class as the
plotters fix in sub-project 1). Diameter range check applies as for other forms.

### Material fields (extended)
Existing: name, specification, mts (equation), youngs_modulus, shear_modulus,
density, allowable_pct_torsion/bending/set, endurance (optional), citations.
Added:
- `max_service_temperature: Option<Temperature>` — informational only, **not used
  in any calculation**; carries a value + unit (°C/°F) and is cited. The UI labels
  it as informational so it is not mistaken for a derating input.
Provenance (curated vs user, for read-only enforcement and UI badging) is **not**
a per-`Material` field. It is derived from `MaterialStore` membership via
`is_curated(name)`: a material in the curated set is curated, one in the user
overlay is user. This keeps `Material` free of store-coupling and makes the
disjoint-sets identity rule (§5) the single source of truth.

### Fallible parsing (fix deferred panic)
`From<RawMaterial>` (which panics on unknown `mts_form`/`mts_units`) becomes
`TryFrom<RawMaterial>` (or a `fn try_from_raw -> Result<Material>`). Unknown
form/unit strings, malformed coefficients, or invalid ranges yield
`SpringError::DataFile`. The bundled curated file is still trusted (a parse
failure there is a build-time bug caught by tests), but the **user overlay is
untrusted** and parsing errors must be handled (§6).

## 5. Storage & identity/merge rule (architectural crux)

- **Name is the unique key** (exact, case-sensitive).
- **Curated names are reserved.** A user material may not reuse a curated name;
  add/save rejects it with a clear `InconsistentInputs` error. The curated and
  user sets are therefore always disjoint — no shadowing, no override.
- **Clone-then-edit:** cloning a curated material creates a new *user* material
  with a distinct default name (e.g. `"<name> (copy)"`); the user renames it.
- **Rename = delete + add** (name is the key).
- **Read-only enforcement:** curated materials cannot be edited or removed (the
  store rejects mutations targeting a curated name); user materials can.
- **Merge at load** = curated ∪ user. A user-file entry whose name collides with a
  curated name (e.g. a hand-edited file) is **rejected with a warning and
  skipped**, never overriding curated data.

`MaterialStore` public API (illustrative):
`load() -> (MaterialStore, Vec<LoadWarning>)`, `names()`, `get(name)`,
`is_curated(name)`, `add(Material) -> Result<()>`, `update(name, Material) -> Result<()>`,
`remove(name) -> Result<()>`, `clone_material(name) -> Result<Material>`,
`save() -> Result<()>` (persists user overlay only).

## 6. Persistence

- **Location:** platform config dir via the `directories` crate
  (`ProjectDirs::from(...)` → e.g. `~/.config/OpenSpringmaker/materials.toml` on
  Linux, the OS equivalent elsewhere).
- **Format:** TOML, same native-unit conventions as the bundled file, with a
  top-level **`schema_version`** integer for forward compatibility.
- **Atomic write:** serialize to a temp file in the same dir, then rename over the
  target, so an interrupted save never corrupts the overlay.
- **Graceful failure (no crash on untrusted input):**
  - missing overlay file → empty user set (normal first-run);
  - unreadable/malformed file or a bad entry → the store loads **curated-only**
    and returns a `LoadWarning` the GUI surfaces; the app never panics on startup
    (same principle as the sub-project 1 crash fix).

## 7. Curated set (~20+) and the data-correctness contract

The dominant risk in this sub-project is **data correctness**: ~20 materials ×
~10 cited values ≈ ~200 numbers. Mutation testing and the review panel verify
*engine logic*, not whether a constant is right — a wrong digit silently becomes a
passing test. Therefore:

- **Two-source rule:** every material value is cross-checked against **two
  independent authoritative sources** and cited inline. Primary: Shigley
  *Mechanical Engineering Design* Tables 10-4 (A, m) and 10-5 (E, G); secondary:
  the SMI *Handbook of Spring Design* and/or the relevant ASTM spec or a
  manufacturer datasheet. Both citations recorded in the data file.
- **Per-material golden test:** each curated material gets an integration-test
  assertion that its MTS at a stated reference diameter equals the published
  figure (an external oracle per material), plus spot-checks on E/G/density.
- **No data certification by mutation score:** a wrong constant is not detectable
  by `cargo-mutants`; the two-source cross-check + per-material golden is the
  contract. Transcription work is reviewed against the sources, not certified by
  "0 missed mutants."

**Form/source per material (sourcing reality):** Shigley's `A/d^m` power-law
constants exist only for the common steels — A227 (hard-drawn), A228 (music),
A229 (oil-tempered), A232 (chrome-vanadium), A401 (chrome-silicon), and 302
stainless. Non-ferrous and precipitation-hardening materials (phosphor bronze,
beryllium copper, Inconel X-750, 17-7 PH, etc.) have **no** Shigley A/m constants
and use the **Constant** form (tabulated UTS) or another curve from ASTM /
manufacturer data. The implementation plan will enumerate, per material, the MTS
form chosen and the two sources used. Endurance data is steel-specific (Zimmerli);
most non-steel materials will have **no** endurance data and correctly report
"no fatigue data" (`FatigueStatus::NoData`).

Target set (~20, final list fixed in the plan with sources): A227, A228, A229,
A230, A232, A401, AISI 1065/1075/1095 carbon steels, 302 & 316 stainless,
17-7 PH stainless, phosphor bronze (B159), beryllium copper (B197), Inconel 600,
Inconel X-750, monel, plus the existing four.

## 8. GUI materials editor

A Materials screen, consistent with the sub-project 1 "engineering-instrument"
visual language (panels, tokens, accent):
- A scrollable **list** of materials with a **curated vs user badge** and a
  read-only lock indicator on curated entries.
- **Add**, **Clone** (from any material → new user material), **Edit** (user only),
  **Remove** (user only), and **Save**.
- An **edit form** with: name, specification, MTS form selector (showing exactly
  the coefficient fields for the chosen form), native units, valid diameter range,
  E, G, density, allowable %s, optional endurance, optional max service temperature
  (labelled informational), and citation text.
- **Live validation** with clear messages (reserved-name, denominator-zero,
  out-of-order diameter range, non-finite/negative values), surfaced like the
  calculator's status panel.
- The calculator's existing material picker reads the merged store; any startup
  `LoadWarning` is shown to the user.
- Pure form-to-material logic separated from iced for unit-testability (mirrors
  `form.rs`).

## 9. Error handling

- No panics on untrusted input: overlay parse failures → `DataFile` error +
  curated-only fallback + surfaced warning.
- Rational denominator guard → `InconsistentInputs`.
- Store mutations enforce invariants (reserved names, read-only curated, valid
  fields) and return typed errors; the editor renders them.

## 10. Testing strategy

- **Engine:** `MaterialStore` CRUD; reserved-name rejection; clone semantics;
  read-only enforcement; merge with disjoint sets; overlay TOML round-trip;
  malformed-file → curated-only fallback (no panic); atomic-save behavior;
  `MtsForm::Rational` evaluation including the denominator guard (unit test with a
  known curve, since no curated material may use Rational); `TryFrom`/parse error
  paths.
- **Data (the contract):** per-material golden MTS cross-checks + E/G/density
  spot-checks against the cited sources.
- **GUI:** pure form-to-material logic tests (validation, form↔material mapping).
- **Mutation gate** remains on engine *logic* (`cargo-mutants` over springcore
  src), not on the data values.
- Strict TDD throughout; tests written first.

## 11. Delivery — three PRs

To keep review tractable and isolate the data-entry risk (per the advisor):
- **PR (a):** data model (`Rational`, temperature, membership-based provenance),
  fallible parsing, `MaterialStore` (CRUD + merge/identity), overlay persistence.
  Engine only; the bundled set may stay at 4 materials here.
- **PR (b):** the ~20+ curated material transcription with two-source citations +
  per-material golden tests.
- **PR (c):** the GUI materials editor.

Each PR: TDD, full gate (fmt, clippy `-D warnings`, tests), mutation gate on
engine logic, adversarial review panel to convergence, CI green before merge.

## 12. Dependencies

- `directories` — config-dir resolution for the user overlay.
- (`toml`, `serde` already in use.)

## 13. References

Shigley's *Mechanical Engineering Design* (Tables 10-4, 10-5); SMI *Handbook of
Spring Design*; relevant **ASTM** specs (A227/A228/A229/A230/A232/A401, A313,
B159, B197, etc.) and manufacturer datasheets for non-ferrous / PH alloys;
Zimmerli (steel endurance). Specific table/section numbers cited inline per value.
