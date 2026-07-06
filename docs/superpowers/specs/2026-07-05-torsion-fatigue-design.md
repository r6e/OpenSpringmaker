# Torsion Fatigue — Design

**Status:** Approved
**Scope:** `springcore` engine only — a new `torsion/fatigue.rs` module + additive
per-material bending-fatigue data. The LAST torsion phase-1 deferred item. The GUI
fatigue section (M_min/M_max inputs, life selector surface, persistence) is the
follow-up — the final torsion GUI increment.

## Decisions (settled during brainstorming)

1. **Gerber, per the source** (user decision): Shigley §10-12's torsion-spring fatigue
   prescription is followed exactly — Table 10-10 R=0 bending data → Eq. 10-58 Se
   (the Gerber R=0 conversion) → Eq. 10-59 Sa with load-line slope r = Ma/Mm →
   Eq. 10-60 nf = Sa/σa. Compression's Goodman module is a DIFFERENT source pairing
   (Zimmerli shear); per-family source fidelity governs, as with the rate
   denominators and index floors. Example 10-8(c) is the exact golden oracle.
2. **Selectable cycle life** (user decision): Table 10-10 carries both 10⁵ and 10⁶
   columns; the API takes a `CycleLife` parameter and the material data stores both
   fractions. `Million` is the default (conservative; the worked example's value).
3. **Engine-pure enum surface**: `CycleLife` ships without serde/Display/ALL — the
   GUI phase adds that surface (the DiaPolicy precedent, deliberately repeated).

## Non-goals

- No GUI (results section, inputs, persistence of the fatigue selector) — follow-up.
- No compression/extension fatigue changes; no temperature/miscellaneous Marin
  factors (the source states Sr is corrected for size/surface/load type only).
- No shot-peened data rows (bundled springs are as-stress-relieved, not-shot-peened;
  the `peened` flag records provenance like `Endurance` does).
- No materials-editor GUI fields for the new data (GUI phase, alongside whatever the
  editor does for `endurance` today).

## A. Material data (additive)

`springcore/src/material.rs`:

```rust
/// Cited repeated-bending fatigue data for torsion springs (Shigley Table 10-10,
/// Associated Spring; R = 0, KB-corrected, no surging, as-stress-relieved).
/// Values are FRACTIONS of Sut (the `allowable_pct_bending` convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BendingFatigue {
    /// Sr/Sut at 10⁵ cycles.
    pub sr_pct_1e5: f64,
    /// Sr/Sut at 10⁶ cycles.
    pub sr_pct_1e6: f64,
    /// Whether the values are the shot-peened column (bundled data: false).
    pub peened: bool,
}
```

`Material` gains `pub bending_fatigue: Option<BendingFatigue>,`. The plumbing mirrors
`endurance` EVERYWHERE it exists: `MaterialDraft` gains an optional
`BendingFatigueDraft` (same field shapes, MPa-free — fractions), the user-overlay
parse/serialize in `material_persist.rs` handles it, and round-trip/`reject`-style
tests extend accordingly. Missing key → `None` everywhere (additive; old overlay
files unaffected).

**Bundled data (`springcore/data/materials.toml`), not-shot-peened column, matched by
recorded ASTM grade:**

| Material | Grade | sr_pct_1e5 | sr_pct_1e6 |
|---|---|---|---|
| Music Wire | A228 | 0.53 | 0.50 |
| Stainless 302 | Type 302 | 0.53 | 0.50 |
| Chrome-Vanadium | A231/A232 range | 0.55 | 0.53 |
| all others (Oil-Tempered **A229**, Chrome-Silicon A401, Hard-Drawn MB A227, Phosphor Bronze) | — | `None` | `None` |

The Chrome-Vanadium entry carries a provenance note: Table 10-10's column is
"ASTM A230 and A232"; our entry's citations already place it in the A231/A232 range
with A232 noted as the valve-spring-quality variant — the data note records this
judgment. Oil-Tempered is A229 (NOT A230) — deliberately `None`; a comment at the
entry says why, so a future reader doesn't "fix" it.

## B. API (`springcore/src/torsion/fatigue.rs`)

```rust
/// Cycle-life class for Table 10-10's two data columns.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleLife {
    /// 10⁵ cycles.
    HundredThousand,
    /// 10⁶ cycles (default — conservative, the worked example's column).
    #[default]
    Million,
}

/// Torsion-spring fatigue analysis per Shigley §10-12 (Gerber, Table 10-10 data).
#[derive(Debug, Clone, Copy)]
pub struct TorFatigueResult {
    /// σa = K_bi·32·Ma/(π·d³) (Eq. 10-44 at the alternating moment).
    pub alternating_stress: Stress,
    /// σm at the mean moment.
    pub mean_stress: Stress,
    /// Fully-reversed endurance Se (Eq. 10-58, Gerber R=0 conversion of Sr).
    pub fully_reversed_endurance: Stress,
    /// Sut(d) — the Gerber ultimate (bending: TENSILE, unlike compression's shear).
    pub ultimate_tensile: Stress,
    /// Gerber strength amplitude Sa (Eq. 10-59, load line r = Ma/Mm).
    pub strength_amplitude: Stress,
    /// nf = Sa/σa (Eq. 10-60).
    pub gerber_factor_of_safety: f64,
}

pub fn analyze_torsion_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    moment_min: Moment,
    moment_max: Moment,
    life: CycleLife,
) -> Result<TorFatigueResult>
```

Re-exports from `torsion/mod.rs`: `analyze_torsion_fatigue`, `CycleLife`,
`TorFatigueResult` (module `mod fatigue;`).

**Validation order (each message pinned):**
1. `validate_wire_mean_geometry(wire_dia, mean_dia)` — the shared design.rs guard
   (error precedence: geometry first, solve_forward's exact messages).
2. `bending_fatigue` data present, else `NoFatigueData(material.name)` —
   compression's exact degradation path.
3. Both moments finite and ≥ 0 → "cycle moments must be finite and non-negative
   (the R = 0 bending data covers unidirectional winding loads)".
4. `moment_max ≥ moment_min` → "max cycle moment must be at least the min cycle
   moment" (compression's message shape).
5. `moment_max > moment_min` (σa > 0) → "cycle moments must differ (a zero
   alternating moment has no fatigue amplitude)" — REQUIRED here unlike
   compression: Gerber's nf = Sa/σa divides by σa (Goodman's reciprocal form
   tolerates τa = 0; Eq. 10-60 does not). Note: this also excludes the both-zero
   pair, so Mm > 0 holds wherever r = Ma/Mm is computed.
6. Eq. 10-58 latent-trap guard (compression's Ssm≥Ssu style): if `Sr/2 ≥ Sut` the
   denominator `1 − (Sr/2/Sut)²` is ≤ 0 — impossible for Table-10-10 fractions
   (≤ 0.64) but guarded with a named `InconsistentInputs` rather than producing a
   negative/∞ Se silently.

**Computation (all via existing cited helpers):** Ma = (Mmax−Mmin)/2,
Mm = (Mmax+Mmin)/2; σa = `bending_stress_inner(Ma, mean_dia, wire_dia)`, σm
likewise (K_bi = Eq. 10-43 = `kbi_factor` — the source prescribes Ki, no selectable
correction); Sut = `min_tensile_strength(wire_dia)` (out-of-range → the existing
`DiameterOutOfRange`); Sr = pct(life)·Sut; Se per Eq. 10-58; r = Ma/Mm; Sa per
Eq. 10-59 (`Sa = (r²·Sut²)/(2·Se) · (−1 + √(1 + (2·Se/(r·Sut))²))`); nf = Sa/σa.

## C. Testing (mutation-gated, literal 0 survivors in-diff)

- **Example 10-8(c) golden (exact)**: Music Wire, d = 0.072 in, D = 0.5218 in,
  Mmin = 1 lbf·in, Mmax = 5 lbf·in, `Million` → assert against the textbook chain
  (σa ≈ 60,857 psi; Se ≈ 78.51 kpsi; Sa ≈ 68.85 kpsi; nf ≈ 1.13) at
  textbook-rounding tolerance (relative ~5e-3), PLUS full-precision self-consistency
  asserts (nf == Sa/σa exactly; σm/σa == Mm/Ma) that pin the algebra tighter than
  the rounded oracle.
- **Life monotonicity + both columns**: same inputs at `HundredThousand` vs
  `Million` → Sr fraction 0.53 vs 0.50 → strictly higher Se/Sa/nf at 10⁵ (Music
  Wire); the Chrome-Vanadium 0.55/0.53 column exercised with its own case.
- **NoFatigueData**: Oil-Tempered Wire (A229, deliberately data-less) → the named
  error; asserts the material name in the message.
- **Guards**: each of §B's messages pinned (min>max; negative; NaN/Inf; equal
  moments incl. the both-zero pair; the Eq. 10-58 trap via a hand-built material
  test fixture with an absurd fraction — a UNIT-level boundary, the established
  pattern for unreachable-through-data traps).
- **Geometry precedence**: wire = 0 with bad moments → the wire message first.
- **Material plumbing**: draft/overlay round-trip with `bending_fatigue` present
  and absent; bundled-data spot checks (the three materials' fractions; the A229
  comment exists).
- Gates: local CI-parity + in-diff mutation (0 survivors); final panel — floor 3 +
  MANDATORY input-domain adversary (numerical attention on the Gerber algebra:
  Eq. 10-59's cancellation behavior for large/small r, the √ argument's domain) +
  persistence/wire-format reviewer (materials.toml + user-overlay draft plumbing is
  a persisted-format surface).

## Task shape (for the plan)

1. Material data layer: `BendingFatigue` + `Material`/draft/overlay plumbing +
   bundled toml data (with the A229/Cr-V provenance notes) + plumbing tests.
2. `torsion/fatigue.rs`: `CycleLife` + `TorFatigueResult` + `analyze_torsion_fatigue`
   (guards → computation) + the golden/monotonicity/guard test set + re-exports +
   full gate.
