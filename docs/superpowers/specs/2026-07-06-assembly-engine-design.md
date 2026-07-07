# Compression-Spring Assemblies — Engine Design

**Status:** Approved
**Scope:** `springcore` engine for a NEW family — assemblies of 1..N
cylindrical round-wire compression springs, topology **Nested** (concentric,
parallel-acting) or **Series** (stacked) — plus additive persistence
(`DesignSpec::Assembly`, the format's FIRST nested-struct list) and the
springmaker placeholder arm. The GUI (dynamic member list, per-member
material pickers) is the follow-up increment with its own spec. Second
compression-variants increment (conical → ASSEMBLIES → rectangular
[source-gated] → variable pitch [parked]).

## Decisions (settled during brainstorming)

1. **Both topologies in v1** (user decision): Nested and Series share ~90% of
   the machinery.
2. **Per-member materials** (user decision): each member carries
   `material_name`, resolved from the `MaterialSet` at solve time. The
   top-level `SavedDesign.material` stays (schema untouched); for assemblies
   it records the app's active picker state at save — MEMBER materials govern
   the solve. This semantic is pinned by tests.
3. **Dynamic member count** (user decision): the engine takes
   `Vec<AssemblyMember>` (1..N); the GUI increment builds the codebase's
   first dynamic add/remove-row widget.
4. **Pure composition**: zero new per-spring physics. Each member is solved
   by the EXISTING cited compression `solve_forward`; only the combination
   layer (rates, shares, travel limits, clearance) is new, each formula
   cited or derived in-code.
5. **The honest boundary, third appearance**: nested members must share a
   free length — staged engagement (members engaging at different
   deflections) is progressive-contact physics, the same class excluded for
   variable pitch and conical's post-bottoming regime. Validation error, not
   a model.
6. **Documented omissions** (module docs; none fabricated): opposite-hand
   winding convention for adjacent nested members (industry practice but not
   in Shigley — omitted, not fabricated); stack-level buckling for series
   (per-member `buckling_stable` flags still surface, member-indexed);
   assembly-level surge/natural frequency.
7. **Placeholder rejection signal returns**: `apply_saved` reverted to `()`
   when conical shipped; this increment's placeholder needs the
   reject-without-recompute behavior again (the error-wipe lesson from the
   conical R2 panel). The signal is reintroduced mechanically — exactly the
   reckoning the conical panel's architect predicted the exhaustive
   `DesignSpec` match would force. The placeholder test drives the REAL
   load→recompute path (not `apply_saved` in isolation).

## Sources

- **Parallel/nested combination**: Shigley 10th ed., Ch. 4's worked
  statically-indeterminate example IS the nested helical pair:
  `F₂ = k₂F/(k₁+k₂)`, `F₁ = k₁F/(k₁+k₂)`, equal deflections,
  `k = k₁ + k₂` — load sharing by rate fraction is the textbook's own
  result, generalized to N members by the same equilibrium argument
  (in-code derivation note).
- **Series combination**: Eq. 8-15, `1/k = 1/k₁ + 1/k₂` "for two springs in
  series", citing Prob. 4-1; generalized to N (in-code note). Equal force
  through members; deflections sum.
- **Nesting endorsement**: §10-1 ("the use of nested round-wire springs
  should always be considered").
- **Everything per-member**: the existing cited compression engine
  (`solve_forward`, Table 10-1 end types, index cautions, allowables).
- **Derived-geometric checks** (in-code derivations, the conical
  precedent): adjacent-member radial interference; travel limits (nested
  bottoms at deflection `L₀ − max(Lsᵢ)`; series bottoms at force
  `min(kᵢ·(L₀ᵢ − Lsᵢ))` — the first member to reach solid).

## A. Engine (`springcore/src/assembly/`, mutation-gated 0 in-diff survivors)

`mod.rs` (module docs: composition model, the staged-engagement boundary,
Decision-6 omissions) + `design.rs`. Re-exports: `Topology`,
`AssemblyMember`, `AssemblyInputs`, `MemberResult`, `AssemblyDesign`,
`solve_assembly`, `evaluate_status`.

```rust
/// Assembly topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    /// Concentric (parallel-acting) members: equal deflections, load shared
    /// by rate fraction (Shigley Ch. 4's nested-pair result), k = Σkᵢ.
    Nested,
    /// Stacked members: equal force through each, deflections sum,
    /// 1/k = Σ 1/kᵢ (Eq. 8-15 / Prob. 4-1).
    Series,
}

/// One member's definition (geometry + its own material).
#[derive(Debug, Clone)]
pub struct AssemblyMember {
    pub material_name: String,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub active_coils: f64,
    pub free_length: Length,
    pub end_type: EndType,
}

/// Assembly inputs: topology + 1..N members. Loads, fixity (one set of end
/// plates), and correction are solve parameters, assembly-wide.
#[derive(Debug, Clone)]
pub struct AssemblyInputs {
    pub topology: Topology,
    pub members: Vec<AssemblyMember>,
}

/// One solved member with its assembly context.
#[derive(Debug, Clone)]
pub struct MemberResult {
    pub material_name: String,
    /// The member solved at ITS load share (Nested: rate-fraction forces;
    /// Series: the full assembly forces).
    pub design: SpringDesign,
    /// kᵢ/Σk — meaningful for Nested (Series members all carry share 1.0).
    pub share_fraction: f64,
}

/// A solved assembly (linear composition of linear members).
#[derive(Debug, Clone)]
pub struct AssemblyDesign {
    pub topology: Topology,
    pub members: Vec<MemberResult>,
    /// Combined rate: Σkᵢ (Nested) or 1/Σ(1/kᵢ) (Series).
    pub rate: SpringRate,
    /// Nested: the shared member free length. Series: Σ free lengths.
    pub free_length: Length,
    /// Nested: max member solid length. Series: Σ member solid lengths.
    pub solid_length: Length,
    /// The usable travel before the first member bottoms, and the force at
    /// that point (derived-geometric; see Sources). Nested: deflection
    /// L₀ − max(Lsᵢ). Series: force min(kᵢ·(L₀ᵢ − Lsᵢ)).
    pub travel_limit_deflection: Length,
    pub travel_limit_force: Force,
    /// Index (into `members`) of the member that bottoms first.
    pub limiting_member: usize,
    /// Assembly-level state at each applied load: force, deflection
    /// (F/k_assembly), assembly length (free_length − deflection).
    pub load_points: Vec<AssemblyLoadPoint>,
}

/// Assembly-level state at one load (per-member detail lives in
/// `members[i].design.load_points`).
#[derive(Debug, Clone, Copy)]
pub struct AssemblyLoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
}

pub fn solve_assembly(
    materials: &MaterialSet,
    inputs: &AssemblyInputs,
    loads: &[Force],
    fixity: EndFixity,
    correction: CurvatureCorrection,
) -> Result<AssemblyDesign>
```

NOTE the signature divergence from sibling engines (which take `&Material`):
assemblies resolve materials PER MEMBER from the `&MaterialSet` — Decision 2.

**Two-pass composition** (each pass reuses `crate::design::solve_forward`
unchanged):
1. **Pass 1 — validate + rates**: each member solved with EMPTY loads.
   Surfaces every existing per-member validation (geometry, material range,
   free-vs-solid) and yields kᵢ. Member errors are wrapped with a
   member-indexed prefix: `InconsistentInputs(msg)` →
   `InconsistentInputs("member {i+1}: {msg}")`; `DiameterOutOfRange` is
   re-emitted as `InconsistentInputs("member {i+1}: {Display of the
   original}")` — CAVEAT (documented in-code): this loses the GUI's
   unit-aware re-formatting for that variant, so assembly member
   diameter-range errors render with the variant's metric Display. Accepted
   for v1: member attribution beats unit-localized formatting; recorded for
   the GUI increment to revisit.
2. **Pass 2 — member shares**: per-member force lists built from the
   topology (Nested: `Fᵢⱼ = (kᵢ/Σk)·Fⱼ` for each assembly load j; Series:
   the full load list), then each member solved AGAIN with its share list —
   the definitive `MemberResult.design` with real per-member load points,
   at_solid, buckling flag.

**Assembly-level validation (before/around the passes, each message
pinned):**
1. `members` non-empty: `"an assembly needs at least one member"`.
2. Loads finite and ≥ 0 (compression's message verbatim: `"loads must be
   finite and non-negative"`) — checked ONCE at assembly level (pass-2
   member solves see derived shares).
3. Nested equal-free-length: all member free lengths bit-equal after parse
   (mm-level equality; the GUI parses per-member strings so exact equality
   is the honest check): `"nested members must share a free length (staged
   engagement is not modeled)"`.
4. Per-member material resolution failure surfaces the store's error wrapped
   with the member prefix.
5. Output-finiteness guard (the hardening standard): assembly rate, every
   `AssemblyLoadPoint` field, `travel_limit_{deflection,force}` all finite,
   else `"assembly solve produced a non-finite result (inputs exceed the
   representable range)"`.

**`evaluate_status(design: &AssemblyDesign, materials: &MaterialSet) ->
DesignStatus`:**
- **Nested clearance** (geometric): members sorted by mean diameter;
  for each adjacent pair (inner i, outer j):
  `members[i].design.outer_dia ≥ members[j].design.inner_dia` → Warning
  `"members {i+1} and {j+1}: nested interference — the inner member's outer
  diameter meets or exceeds the outer member's inner diameter"` (≥ pinned).
  Series: no clearance checks.
- **Per-member engineering status, member-prefixed**: re-run compression's
  per-load overstress and at-solid checks per member (via the member's own
  material's allowables), prefixing each message `"member {i+1}: "`; the
  shared index caution per member likewise (`index_caution_labeled` with
  `"member {i+1} spring index"`).
- **Per-member buckling**: `!design.buckling_stable` → Warning
  `"member {i+1}: free length exceeds the absolute-stability limit; buckling
  possible"` (compression's message, member-prefixed).
- **Travel-limit exceeded**: any assembly load point whose deflection
  exceeds `travel_limit_deflection` → Warning `"load point {j+1} exceeds the
  travel limit (member {limiting+1} bottoms first)"`.
- NO stack-buckling or surge status (Decision 6).

## B. Persistence (additive; the format's first nested-struct list —
persistence reviewer on the panel)

```rust
pub enum DesignSpec {
    Compression(ScenarioSpec),
    Extension(ExtScenarioSpec),
    Torsion(TorsionSpec),
    Conical(ConicalSpec),
    Assembly(AssemblySpec),   // NEW
}

/// Assembly scenarios (v1: direct geometry only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AssemblySpec {
    PowerUser {
        /// "nested" | "series" (parse_topology, the parse_end_type pattern).
        topology: String,
        fixity: String,
        members: Vec<AssemblyMemberSpec>,
        loads_n: Vec<f64>,
    },
}

/// One persisted member. Every field required (structural forward-compat:
/// a misspelled key errors as missing, per the file's convention).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssemblyMemberSpec {
    pub material_name: String,
    pub end_type: String,
    pub wire_dia_mm: f64,
    pub mean_dia_mm: f64,
    pub active: f64,
    pub free_length_mm: f64,
}
```

- `parse_topology(&str) -> Result<Topology>` mirroring `parse_end_type`
  (unknown → `DataFile("unknown topology: {s}")`), promoted alongside it.
- `reject_non_finite` walks the value tree — covers member-struct floats and
  the loads array with no changes (test-pinned anyway).
- `solve_with_material` gains the sibling REJECTION arm:
  `"SavedDesign::solve handles compression designs; assembly designs are
  solved via the assembly scenario"`.
- Decision-2 semantic: `SavedDesign.material` is untouched schema-wise;
  for `DesignSpec::Assembly` files it records the active picker state and
  is NOT consulted by the assembly solve. Stated in a doc comment at the
  `Assembly` variant and pinned by a test (an assembly file whose top-level
  material differs from every member's still solves per member).

## C. springmaker placeholder (compile-wiring + the rejection signal)

Per Decision 7: `apply_saved` regains a rejection signal —
`fn apply_saved(&mut self, saved: SavedDesign) -> bool` (the mechanical
reintroduction the conical panel predicted), with the `Assembly` arm as the
ONLY `false` path: it sets `action_error` to `"assembly designs are not
supported by this build yet (the assembly GUI ships in the next increment)"`
BEFORE any state mutation (the wholesale-reject shape), and `load_from`'s Ok
arm forwards the bool so no recompute wipes the error. The four real
families return `true`. The spec/doc comments record the signal's return.
The placeholder test drives the REAL path: `load_from` on an assembly TOML
file + recompute-if-true → the error survives, material/unit_system
unchanged. Do NOT add `Family::Assembly` (the GUI increment's job).

## D. Testing (mutation-gated)

- **Identity oracles** (the conical zero-taper pattern): a ONE-member
  assembly of either topology matches the bare compression `solve_forward`
  on that member exactly (rate, load points, solid length, at_solid,
  travel limit = the member's own) at 1e-12; TWO IDENTICAL members: Nested
  → rate 2k, each share 0.5, member forces F/2; Series → rate k/2, member
  forces F, assembly deflection 2·(F/k); both closed-form.
- **Ch. 4 share pinning**: two UNEQUAL members nested — shares equal
  kᵢ/Σk exactly; member deflections all equal the assembly deflection
  (the equal-deflection constraint, 1e-12).
- **Travel limits**: nested — the max-Ls member is `limiting_member`, the
  limit deflection L₀ − max(Ls) exact; series — the member with min
  kᵢ(L₀ᵢ−Lsᵢ) limits; boundary: a load exactly AT the limit does not warn,
  just above does (pinned semantics).
- **Clearance**: adjacent-pair interference exactly-at (≥ → warn) and just
  clear; three-member chains check ADJACENT pairs only — with members sorted
  by mean diameter, a non-adjacent interference implies an adjacent one, so
  adjacent-only is the complete geometric contract (in-code derivation note;
  pinned by a three-member test); series never warns.
- **Guard matrix**: empty members; nested unequal free lengths (message
  pinned); unknown member material (member-prefixed); per-member geometry
  errors carry the member prefix (and the DiameterOutOfRange re-emission
  caveat pinned); output guard via a 1e305 load.
- **Decision-2 semantic**: top-level material ≠ any member's → solves per
  member (pinned).
- **Persistence**: round-trip with 1 and 3 members; rejects — missing field
  INSIDE a member struct, non-finite inside a member, non-finite in
  loads_n, unknown topology, empty members array (engine-level reject; the
  file itself parses); the placeholder real-path test (§C).
- **Gates**: local CI-parity + in-diff mutation 0 survivors; final panel —
  floor 3 + MANDATORY input-domain adversary (topology × member-count ×
  degenerate-member matrix) + PERSISTENCE reviewer (first nested-struct
  list in the format).

## Task shape (for the plan)

1. `springcore/src/assembly/` — types/solve_assembly/evaluate_status + the
   full engine test set + mutation gate.
2. Persistence (AssemblySpec + AssemblyMemberSpec + parse_topology +
   dispatch rejection + round-trip/reject tests) + the springmaker
   placeholder (bool signal reintroduction + real-path test) + full gate.
