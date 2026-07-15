# Inactive Coil Count — Design Specification

**Date:** 2026-07-14  
**Author:** Claude Code  
**Scope:** Compression, Conical, Assembly member springs  
**Status:** Approved by user, pending implementation plan

---

## Executive Summary

Add user-specifiable control over the inactive (dead) coil count in helical springs. Today, the count is locked to the end-type default (0/1/2 per Shigley Table 10-1). Springs often have a different number of dead coils (seating coils, tang allowances, etc.), and this feature makes that count an input.

- **Single total count** (`inactive_coils: Option<f64>`) per spring or assembly member
- **Fully backward-compatible:** `None` defaults to the end-type value; existing designs are byte-identical
- **Applies to** Compression (all 5 input modes), Conical (PowerUser), Assembly members
- **Renderer:** no changes (already draws dead coils from total-active split)
- **Rate/stress/frequency:** unchanged by inactive count (only geometry/mass/length affected)

---

## Design Approach: Additive Closed-Coil Override

### Model

A dead coil is wound tight (pitch = wire diameter `d`), occupies exactly `d` of axial height, contributes 0 to deflection. The override is:

```
Ni = inactive_coils input (user-specified)
Ne = end_type.end_coils() (Shigley default: Plain=0, PlainGround=1, Squared/SquaredGround=2)
Nt = Na + Ni  (total coils)

Free length = <current formula for end type> + (Ni - Ne) * d
Solid length = d * Nt  (ground) or d * (Nt + 1)  (non-ground)
Pitch = end_type.pitch_from_free_length(free - (Ni - Ne) * d)
```

Extra dead coils beyond the default are **closed** (pitch = d, each adding `d` to free and solid length).

### Backward Compatibility

At the default `Ni = Ne`:
- `free(pitch=d) = old_free(d) + 0 = old_solid` — the tight-wound invariant holds exactly
- `total = Na + Ne = today's formula` — byte-identical
- Every existing solver call succeeds; guard behavior (`free ≥ solid`) is unchanged
- Rate, stress, natural frequency are unaffected (depend only on `active`, not total)

**Consequence:** Loading an old saved file produces the exact same design. The feature is additive, not transformative.

### Special case: PlainGround open-ground coil

PlainGround's single default inactive coil is modeled at open pitch `p` (per Shigley), not `d`. The additive override preserves this:
- `Ni = 1` (default): one open-pitch ground coil (no change)
- `Ni > 1`: the first coil stays open-pitch; additional dead coils are closed (pitch = `d`)

This is rare in practice but physically defensible—a user specifying 2–3 dead coils on PlainGround gets the standard ground coil plus extra seating coils (closed).

---

## Implementation Sites

### Engine (`springcore/src`)

**`end_type.rs`** — generalize four methods to take explicit `inactive: f64`:

```rust
pub fn total_coils(self, active: f64, inactive: f64) -> f64 { active + inactive }

pub fn solid_length(self, wire_dia: Length, active: f64, inactive: f64) -> Length {
  let total = self.total_coils(active, inactive);
  match self {  // EndType has no is_ground(); the ground split is an explicit match (see line 42 today)
    Self::PlainGround | Self::SquaredGround => wire_dia * total,        // ground
    Self::Plain | Self::Squared            => wire_dia * (total + 1.0), // non-ground
  }
}

pub fn free_length(self, wire_dia: Length, active: f64, pitch: Length, inactive: f64) -> Length {
  let base = <current per-end formula>; // e.g. Plain: pitch * active + wire_dia
  let ne = self.end_coils();  // default for this end type
  base + wire_dia * (inactive - ne)
}

pub fn pitch_from_free_length(self, wire_dia: Length, active: f64, free_length: Length, inactive: f64) -> Length {
  let ne = self.end_coils();
  let adjusted_free = free_length - wire_dia * (inactive - ne);
  <current per-end formula, applied to adjusted_free>
}
```

The `Option → concrete` resolution lives at **one place per solver entry:**
```rust
let inactive = inactive_coils.unwrap_or(end_type.end_coils());
```

**`design.rs`** — `solve_forward` signature:
```rust
pub fn solve_forward(
  material: &Material,
  end_type: EndType,
  fixity: EndFixity,
  wire_dia: Length,
  mean_dia: Length,
  active: f64,
  inactive: f64,  // <-- added (concrete, already resolved from Option)
  free_length: Length,
  loads: &[Force],
  correction: CurvatureCorrection,
) -> Result<SpringDesign>
```

Body updates:
- Line 141: `let total_coils = end_type.total_coils(active, inactive);`
- Line 141: `let solid_length = end_type.solid_length(wire_dia, active, inactive);`
- Line 158: `let pitch = end_type.pitch_from_free_length(wire_dia, active, free_length, inactive);`
- Guard (lines 146–153) is unchanged; `free ≥ solid` naturally rejects over-specified dead coils.

**`scenario.rs`** — the four scenario structs `PowerUser`, `TwoLoad`, `RateBased`,
`Dimensional` (each `impl Scenario`, each calls `solve_forward` exactly once — at
lines 32, 86, 128, 164 today) each add:
```rust
pub inactive_coils: Option<f64>,
```

At each `solve_forward` call (exactly one site per struct):
```rust
let inactive = self.inactive_coils.unwrap_or(end_type.end_coils());
solve_forward(..., self.end_type, ..., active, inactive, free_length, ...)
```

**`optimize.rs`** — the fifth mode, MinWeight, lives here (not in `scenario.rs`).
`MinWeightRequest` already carries `end_type: EndType`; add
`pub inactive_coils: Option<f64>` and resolve `inactive` at **two** sites: the
per-candidate `solid = req.end_type.solid_length(d, active, inactive)` feasibility
calc and the final `solve_forward` call. `Ni` is a fixed input, never a search
variable; mass auto-scales because `wire_mass(..., design.total_coils)` keys off
`total_coils`, which now grows with `Ni`.

**`conical/design.rs`** — `solve_forward` parallel update (conical carries `end_type` for ground geometry; dead coils affect `solid_length` + `telescopes` derivation).

**`assembly/design.rs`** — `AssemblyMember` / member-solve paths thread `inactive` per member (each member is a compression spring with its own `end_type` and now `inactive_coils`).

### Persistence (`springcore/src/persistence.rs`)

Add `inactive_coils: Option<f64>` to:
- `ScenarioSpec` (all 5 variants: PowerUser, TwoLoad, RateBased, Dimensional, MinWeight)
- `ConicalSpec::PowerUser`
- `AssemblyMemberSpec`

Serde defaults missing keys to `None` → end-type default (exact pattern as `arbor_dia_mm`).

### GUI (`springmaker/src`)

**Compression form** (all 5 input modes):
- Add "Inactive coils" text input (optional, empty → placeholder = end-type default)
- Label: "Inactive coils per end (default: 0/1/2 for Plain/PlainGround/Squared)"
- Update form struct: `pub inactive_coils: Option<f64>,`
- Update `populate_from_spec` and `parse_and_solve` to pass `inactive` (resolved from `Option`)

**Conical form** (PowerUser only):
- Same "Inactive coils" input (note: conical is rare with ground, mostly for geometry variation)

**Assembly form** (member row):
- Add "Inactive coils" column (optional per member)
- Each member's row resolves its `inactive` at the form level

Update results panels (compression, conical, assembly) to display "Total coils" as a derived field (already exists in most; verify `total_coils` is wired).

### Testing

Strict TDD per CLAUDE.md (unit tests in `springcore/src/end_type.rs`,
`design.rs`, `scenario.rs`, `optimize.rs`; GUI tests in `springmaker/src`; for
each end type and each input mode):

1. **Backward-compat lock:** parameterized test over all 4 end types. For each, construct a design with `Ni = None` (default) and one with `Ni = Ne` (explicit). Assert byte-identical outputs: `total_coils`, `solid_length`, `pitch`, `rate`, `natural_frequency`, all `load_points[*].stress`.

2. **Tight-wound invariant:** for all 4 end types, assert `free_length(pitch=d) == solid_length` when `Ni = Ne`. Algebraic proof + test fixtures.

3. **Additive term:** for each end type, set `Ni = Ne + 1`, and verify `free_new = free_old + d`, `solid_new = solid_old + d`. Repeat for `Ni = Ne + 2, Ne + 2.5`.

4. **Mass scaling:** for a fixed design, `mass(Ni + 1) = mass(Ni) + wire_mass(..., 1 extra coil)`.

5. **Rate/stress invariance:** for all end types and all input modes, verify rate/stress are unchanged by `Ni` (both depend only on `active`, not `total`).

6. **Persistence round-trip:**
   - `SavedDesign` with `inactive_coils: None` → TOML → deserialize → `inactive_coils: None` (invariant).
   - `SavedDesign` with `inactive_coils: Some(3.0)` → round-trip → exact match.
   - Old saved file (no `inactive_coils` key) → deserialize → `None` (backward-compat).

7. **Derived-mode feasibility:** MinWeight solver must not reject a feasible solution just because `Ni ≠ Ne`; test with a non-default `Ni` and confirm a solution is found.

8. **Guard behavior:** `free < solid` guard triggers naturally when dead coils push `solid_length` above a user-specified `free_length` (i.e. over-constraint is impossible to hide).

Conical mirror: same suite applied to conical (single PowerUser mode), including its `solid_length` + `telescopes` derivation.

Assembly mirror: test both nested and series topologies, confirming per-member `inactive` is honored and travel limits / bottoming math remain correct.

---

## Consumers and Invariants

### No changes required

- **Renderer** (`springmaker/src/viz/mod.rs`): draws `(total − active) / 2` dead coils per end at pitch = `d`. Already correct.
- **3D SDF** (`viz/sdf.rs`): ground cuts for `*Ground` ends; dead-coil subdivision already in place.
- **2D diagram** (all presenters): uses `total_coils`/`active_coils`, which now reflect `Ni`. Callout anchors unchanged.

### Verification (minimal; enumerate upfront per invariant-change discipline)

- **Results panels:** display `total_coils` as derived (compression/conical already do; assembly per-member verify).
- **Mass/weight displays:** scale with `total_coils` (most already do; verify no hardcoded assumptions).
- **Conical `telescopes` derivation:** depends on `solid_length(active, inactive)` — verify it stays correct. (No solver logic change; just parameterization.)
- **Assembly `travel_limit` / bottoming math:** per-member `solid_length` is used; verify nested's shared-free-length guard (`all_members.free_length == shared_free`) is unaffected. (It's an input constraint, not dependent on `Ni`.)

---

## Notes and Open Questions

1. **Validation:** `inactive_coils` accepts any finite ≥ 0 value, including fractional (e.g. 1.5). No "inactive > active" caution defined (YAGNI; users know their springs). The `free ≥ solid` guard is the enforcement.

2. **Naming:** field is `inactive_coils` in persistence and forms. Engine method signature prefers `inactive` for brevity.

3. **Mass and energy:** inert coils do add mass (they're physical wire). Stored energy in dead coils is 0 (they don't deflect), so they don't contribute to the spring constant or stress — only to geometry, length, and mass. This is modeled correctly (rate depends only on `active`).

4. **Migration:** old saved files load as `None`, resolve to end-type default at solver time. Zero explicit migration logic needed.

5. **Extension/Torsion exclusion:** Extension is close-wound by construction (`Na == Nt`, no dead-coil concept). Torsion is the same (body is all-active). No `inactive_coils` field for these families.

---

## Deliverable

- Spec review: user confirms this captures intent
- Implementation: per the writing-plans skill output
- Acceptance: all tests pass, backward-compat lock holds, PR converges through adversarial panel

---

**Design approved by user:** 2026-07-14 (verbal approval before spec write-up).
**Implementation plan:** to be authored via the writing-plans skill (next step),
saved to `docs/superpowers/plans/2026-07-14-inactive-coils.md`.
