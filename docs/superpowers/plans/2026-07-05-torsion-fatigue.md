# Torsion Fatigue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `springcore::torsion::analyze_torsion_fatigue` — Gerber fatigue analysis for alternating-moment torsion springs per Shigley §10-12, with per-material Table 10-10 bending-fatigue data.

**Architecture:** Task 1 adds the additive material-data layer (`BendingFatigue` mirrored through the raw/draft/toml pipeline exactly where `endurance` lives — all in `material.rs` + `data/materials.toml`). Task 2 adds `torsion/fatigue.rs` (CycleLife, TorFatigueResult, the six-guard validation order, the cited Gerber chain) with Example 10-8(c) as the exact golden oracle, plus re-exports and the full gate.

**Tech Stack:** Rust (MSRV 1.88), approx, cargo-mutants (in-diff gate).

## Global Constraints

- springcore mutation-gated to **literal 0 survivors**: `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features` after every task.
- Strict TDD. Engine-only (no springmaker, no DesignSpec/persistence.rs change — materials.toml + the user-overlay raw record is the only persisted surface, and it is ADDITIVE).
- Citations at every formula: Shigley §10-12; Eq. 10-43 (Ki = `kbi_factor`), Eq. 10-58 (Se), Eq. 10-59 (Sa), Eq. 10-60 (nf); Table 10-10 (Associated Spring — an academic table source, permitted like Shigley/EN).
- `CycleLife` is ENGINE-PURE: `#[non_exhaustive]`, `Million` is `#[default]`, NO serde/Display/ALL const (the GUI phase adds that surface — the DiaPolicy precedent, deliberate).
- **Implementers commit DIRECTLY on `feat/torsion-fatigue` and NEVER push, NEVER create/edit PRs, NEVER run review panels, NEVER touch `.git/REVIEW_CONVERGED_OK`.**
- Golden oracle (Shigley Example 10-8(c), US units): Music Wire, d = 0.072 in, D = 0.5218 in, Mmin = 1 lbf·in, Mmax = 5 lbf·in, `Million` → Ma = 2, Mm = 3, r = 2/3, σa = 60,857 psi, σm = 91,286 psi, Sr = 0.50·Sut = 147.2 kpsi, Se = 78.51 kpsi, Sa = 68.85 kpsi, **nf = 1.13**. PSI→Pa constant for asserts: `6894.757293168`.

---

## File Structure

- Modify `springcore/src/material.rs` — `BendingFatigue`/`BendingFatigueDraft`/raw plumbing (six mirror sites, line anchors below).
- Modify `springcore/data/materials.toml` — three `[material.bending_fatigue]` blocks + the A229 comment.
- Create `springcore/src/torsion/fatigue.rs`; modify `springcore/src/torsion/mod.rs` (re-exports).

---

### Task 1: Material data layer — BendingFatigue through the endurance pipeline

**Files:**
- Modify: `springcore/src/material.rs` (mirror sites: public struct after `Endurance` ~line 130; `Material` field after `endurance` ~242; `MaterialDraft` field after `endurance` ~171 + a `BendingFatigueDraft` after `EnduranceDraft` ~185; `MaterialDraft::build`'s raw mapping ~209; the raw record ~180 and ~382; the Material→raw serialize direction ~283; the finiteness chain ~480; `try_from_raw`'s conversion ~553)
- Modify: `springcore/data/materials.toml`

**Interfaces:**
- Produces (Task 2 consumes): `pub struct BendingFatigue { pub sr_pct_1e5: f64, pub sr_pct_1e6: f64, pub peened: bool }` (derives `Debug, Clone, Copy, PartialEq` — Endurance's derives + PartialEq for tests); `Material.bending_fatigue: Option<BendingFatigue>`; bundled data Music Wire/Stainless 302 = 0.53/0.50, Chrome-Vanadium = 0.55/0.53, all others `None`.

- [ ] **Step 1: Write the failing tests** (material.rs `mod tests`):

```rust
    #[test]
    fn bundled_bending_fatigue_matches_table_10_10() {
        let set = MaterialSet::load_default();
        let cases = [
            ("Music Wire", 0.53, 0.50),
            ("Stainless 302", 0.53, 0.50),
            ("Chrome-Vanadium", 0.55, 0.53),
        ];
        for (name, p5, p6) in cases {
            let m = set.get(name).expect(name);
            let bf = m
                .bending_fatigue
                .unwrap_or_else(|| panic!("{name} carries Table 10-10 data"));
            assert_eq!(bf.sr_pct_1e5, p5, "{name} 10^5 column");
            assert_eq!(bf.sr_pct_1e6, p6, "{name} 10^6 column");
            assert!(!bf.peened, "bundled data is the not-shot-peened column");
        }
        // Grade honesty: Oil-Tempered is ASTM A229 — NOT Table 10-10's A230 column.
        for name in ["Oil-Tempered Wire", "Chrome-Silicon", "Hard-Drawn MB", "Phosphor Bronze"] {
            assert!(
                set.get(name).unwrap().bending_fatigue.is_none(),
                "{name} has no matching Table 10-10 grade"
            );
        }
    }

    #[test]
    fn draft_round_trips_bending_fatigue_present_and_absent() {
        // Draft -> Material -> draft-visible fields, both states. Bind the set
        // first: `get` borrows from it (Result<&Material>), so a temporary set
        // would not live long enough.
        let set = MaterialSet::load_default();
        let mut d = draft_from_material(set.get("Music Wire").unwrap());
        assert!(d.bending_fatigue.is_some(), "draft carries the bundled data");
        let m = d.build().unwrap();
        let bf = m.bending_fatigue.unwrap();
        assert_eq!((bf.sr_pct_1e5, bf.sr_pct_1e6, bf.peened), (0.53, 0.50, false));
        d.bending_fatigue = None;
        assert!(d.build().unwrap().bending_fatigue.is_none());
    }

    #[test]
    fn non_finite_bending_fatigue_fraction_rejected() {
        // The raw finiteness chain must cover the two new fractions.
        let set = MaterialSet::load_default();
        let mut d = draft_from_material(set.get("Music Wire").unwrap());
        d.bending_fatigue = Some(BendingFatigueDraft {
            sr_pct_1e5: f64::NAN,
            sr_pct_1e6: 0.50,
            peened: false,
        });
        assert!(d.build().is_err(), "NaN fraction must fail the build");
    }
```

NOTE: `draft_from_material` — use the module's existing Material→Draft path (the tests module already builds drafts; reuse its established constructor/helper. If the existing tests construct `MaterialDraft` literally, follow that shape instead and load the bundled values into it — match the file's conventions, keeping the three assertions as written).

- [ ] **Step 2: Run to verify fail** — `cargo test -p springcore --lib material` → FAIL (no `bending_fatigue` field).

- [ ] **Step 3: Implement — the six mirror sites** (each mirrors its `endurance` neighbor exactly):

1. Public struct (after `Endurance`):

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

2. `Material` gains `/// Optional cited bending-fatigue data (torsion springs).\n    pub bending_fatigue: Option<BendingFatigue>,` after `endurance`.
3. `MaterialDraft` gains `/// Optional cited bending-fatigue data.\n    pub bending_fatigue: Option<BendingFatigueDraft>,` after `endurance`, plus:

```rust
/// Editable bending-fatigue data within a [`MaterialDraft`]. Fractions of Sut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BendingFatigueDraft {
    pub sr_pct_1e5: f64,
    pub sr_pct_1e6: f64,
    pub peened: bool,
}
```

4. The RAW record (both raw structs, ~180 and ~382) gains an optional sub-table mirroring `RawEndurance`'s shape:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RawBendingFatigue {
    pub(crate) sr_pct_1e5: f64,
    pub(crate) sr_pct_1e6: f64,
    pub(crate) peened: bool,
}
```

with `bending_fatigue: Option<RawBendingFatigue>` fields beside each raw `endurance` field. (Field names are the TOML keys — no `_mpa` suffix: these are dimensionless fractions.)
5. Wire all four conversion/validation sites beside their endurance twins: `MaterialDraft::build`'s raw mapping (~209), the Material→raw serialize direction (~283), the finiteness chain (~480: `.chain(r.bending_fatigue.iter().flat_map(|b| [b.sr_pct_1e5, b.sr_pct_1e6]))`), and `try_from_raw` (~553: map to the public struct).
6. `springcore/data/materials.toml`: add after each `[material.endurance]` block (or at the entry tail where none exists):

```toml
# Shigley Table 10-10 (Associated Spring): R = 0 repeated-bending strength as a
# fraction of Sut, not-shot-peened column ("ASTM A228 and Type 302").
[material.bending_fatigue]
sr_pct_1e5 = 0.53
sr_pct_1e6 = 0.50
peened = false
```

for Music Wire and Stainless 302; for Chrome-Vanadium:

```toml
# Shigley Table 10-10 "ASTM A230 and A232" column. Our entry's citations place it
# in the A231/A232 range with A232 the valve-spring-quality variant — recorded
# provenance judgment.
[material.bending_fatigue]
sr_pct_1e5 = 0.55
sr_pct_1e6 = 0.53
peened = false
```

and at the Oil-Tempered entry (a comment ONLY, no block):

```toml
# NO bending_fatigue: this wire is ASTM A229; Shigley Table 10-10's column is
# "ASTM A230 and A232" — a different grade. Deliberately data-less (torsion
# fatigue degrades to NoFatigueData), not an omission.
```

Export the two new public types from lib.rs beside `Endurance, EnduranceDraft`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib` → PASS.
- [ ] **Step 5: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Kill map: the finiteness-chain extension → non_finite test; conversion-site field
# swaps (1e5↔1e6) → the exact-column asserts in bundled + draft tests.
git add springcore/src/material.rs springcore/data/materials.toml springcore/src/lib.rs
git commit -m "feat(material): Table 10-10 bending-fatigue data — additive, grade-honest"
```

---

### Task 2: `torsion/fatigue.rs` — the Gerber analysis

**Files:**
- Create: `springcore/src/torsion/fatigue.rs`
- Modify: `springcore/src/torsion/mod.rs` (`mod fatigue;` + `pub use fatigue::{analyze_torsion_fatigue, CycleLife, TorFatigueResult};`)

**Interfaces:**
- Consumes: Task 1's `Material.bending_fatigue`/`BendingFatigue`; `design::validate_wire_mean_geometry` (pub(crate)); `mechanics::bending_stress_inner`; `Material::min_tensile_strength`; `SpringError::{InconsistentInputs, NoFatigueData}`.
- Produces: the spec's API verbatim (`CycleLife { HundredThousand, #[default] Million }` `#[non_exhaustive]`; `TorFatigueResult` six fields; `analyze_torsion_fatigue(material, wire_dia, mean_dia, moment_min, moment_max, life) -> Result<TorFatigueResult>`).

- [ ] **Step 1: Write the failing tests** (fatigue.rs created test-module-first — the RED is the compile fail):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::music_wire;
    use crate::units::{Length, Moment};
    use approx::assert_relative_eq;

    /// Pa per psi (exact: 4.4482216152605 N/lbf ÷ 0.00064516 m²/in²).
    const PSI: f64 = 6894.757293168361;

    fn golden(life: CycleLife) -> TorFatigueResult {
        // Shigley Example 10-8(c): music wire, d = 0.072 in, D = 0.5218 in,
        // M cycles 1 → 5 lbf·in.
        analyze_torsion_fatigue(
            &music_wire(),
            Length::from_inches(0.072),
            Length::from_inches(0.5218),
            Moment::from_pound_force_inches(1.0),
            Moment::from_pound_force_inches(5.0),
            life,
        )
        .expect("the worked example is feasible")
    }

    #[test]
    fn shigley_example_10_8c_golden() {
        let r = golden(CycleLife::Million);
        // Textbook-rounded chain at 5e-3 relative (the book rounds intermediates):
        assert_relative_eq!(r.alternating_stress.pascals() / PSI, 60_857.0, max_relative = 5e-3);
        assert_relative_eq!(r.mean_stress.pascals() / PSI, 91_286.0, max_relative = 5e-3);
        assert_relative_eq!(r.fully_reversed_endurance.pascals() / PSI, 78_510.0, max_relative = 5e-3);
        assert_relative_eq!(r.strength_amplitude.pascals() / PSI, 68_850.0, max_relative = 5e-3);
        assert_relative_eq!(r.gerber_factor_of_safety, 1.13, max_relative = 5e-3);
        // Full-precision self-consistency (pins the algebra tighter than the
        // rounded oracle): nf ≡ Sa/σa; σm/σa ≡ Mm/Ma = 3/2.
        assert_relative_eq!(
            r.gerber_factor_of_safety,
            r.strength_amplitude.pascals() / r.alternating_stress.pascals(),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            r.mean_stress.pascals() / r.alternating_stress.pascals(),
            1.5,
            max_relative = 1e-12
        );
    }

    #[test]
    fn hundred_thousand_life_gives_strictly_higher_margin() {
        // Sr fraction 0.53 vs 0.50 (Music Wire) → higher Se, Sa, nf at 10⁵. The
        // ratio Se(1e5)-vs-Se(1e6) pins BOTH columns (kills a column-swap mutant:
        // swapped columns would invert the inequality).
        let m6 = golden(CycleLife::Million);
        let m5 = golden(CycleLife::HundredThousand);
        assert!(m5.fully_reversed_endurance.pascals() > m6.fully_reversed_endurance.pascals());
        assert!(m5.strength_amplitude.pascals() > m6.strength_amplitude.pascals());
        assert!(m5.gerber_factor_of_safety > m6.gerber_factor_of_safety);
        // Stresses are life-independent.
        assert_relative_eq!(
            m5.alternating_stress.pascals(),
            m6.alternating_stress.pascals(),
            max_relative = 1e-12
        );
    }

    #[test]
    fn chrome_vanadium_column_is_used() {
        // 0.55/0.53 (Table 10-10 "A230 and A232" column): Sr at Million must be
        // exactly 0.53·Sut(d) — pins the per-material lookup, not just Music Wire's.
        let set = crate::MaterialSet::load_default();
        let cv = set.get("Chrome-Vanadium").unwrap();
        let d = Length::from_inches(0.072);
        let r = analyze_torsion_fatigue(
            cv,
            d,
            Length::from_inches(0.5218),
            Moment::from_pound_force_inches(1.0),
            Moment::from_pound_force_inches(5.0),
            CycleLife::Million,
        )
        .expect("feasible");
        let sut = cv.min_tensile_strength(d).unwrap().pascals();
        let sr = 0.53 * sut;
        let expected_se = (sr / 2.0) / (1.0 - (sr / 2.0 / sut).powi(2));
        assert_relative_eq!(
            r.fully_reversed_endurance.pascals(),
            expected_se,
            max_relative = 1e-12
        );
    }

    #[test]
    fn material_without_data_degrades_to_no_fatigue_data() {
        let set = crate::MaterialSet::load_default();
        let otw = set.get("Oil-Tempered Wire").unwrap(); // A229: deliberately data-less
        let err = analyze_torsion_fatigue(
            otw,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Moment::from_newton_millimeters(100.0),
            Moment::from_newton_millimeters(500.0),
            CycleLife::Million,
        )
        .expect_err("no Table 10-10 grade match");
        match err {
            crate::SpringError::NoFatigueData(name) => assert_eq!(name, "Oil-Tempered Wire"),
            other => panic!("expected NoFatigueData, got {other:?}"),
        }
    }

    #[test]
    fn guards_fire_in_order_with_pinned_messages() {
        let m = music_wire();
        let (d, dm) = (Length::from_millimeters(2.0), Length::from_millimeters(20.0));
        let mm = Moment::from_newton_millimeters;
        // Geometry precedence: wire = 0 beats bad moments.
        let err = analyze_torsion_fatigue(
            &m, Length::from_meters(0.0), dm, mm(-1.0), mm(-2.0), CycleLife::Million,
        )
        .unwrap_err();
        assert!(err.to_string().contains("wire diameter must be a positive finite number"));
        // Non-negative + finite (the R = 0 domain), covering NaN/Inf/negative:
        for (lo, hi) in [(-1.0, 500.0), (f64::NAN, 500.0), (100.0, f64::INFINITY)] {
            let err = analyze_torsion_fatigue(&m, d, dm, mm(lo), mm(hi), CycleLife::Million)
                .unwrap_err();
            assert!(
                err.to_string().contains(
                    "cycle moments must be finite and non-negative \
                     (the R = 0 bending data covers unidirectional winding loads)"
                ),
                "({lo},{hi}): {err}"
            );
        }
        // Order: max ≥ min.
        let err = analyze_torsion_fatigue(&m, d, dm, mm(500.0), mm(100.0), CycleLife::Million)
            .unwrap_err();
        assert!(err.to_string().contains("max cycle moment must be at least the min cycle moment"));
        // Equal (incl. both-zero) → the Gerber-amplitude guard.
        for v in [300.0, 0.0] {
            let err = analyze_torsion_fatigue(&m, d, dm, mm(v), mm(v), CycleLife::Million)
                .unwrap_err();
            assert!(
                err.to_string().contains(
                    "cycle moments must differ (a zero alternating moment has no fatigue amplitude)"
                ),
                "v={v}: {err}"
            );
        }
    }

    #[test]
    fn absurd_user_material_fraction_trips_the_eq_10_58_trap() {
        // REACHABLE via user-overlay materials: an Sr fraction ≥ 2 makes Eq. 10-58's
        // denominator ≤ 0. Build such a material through the draft path and assert
        // the named trap instead of a silent negative/∞ Se.
        let set = crate::MaterialSet::load_default();
        let mut d = draft_from_material(set.get("Music Wire").unwrap());
        d.name = "Absurd".into();
        d.bending_fatigue = Some(crate::BendingFatigueDraft {
            sr_pct_1e5: 2.5,
            sr_pct_1e6: 2.5,
            peened: false,
        });
        let absurd = d.build().unwrap();
        let err = analyze_torsion_fatigue(
            &absurd,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Moment::from_newton_millimeters(100.0),
            Moment::from_newton_millimeters(500.0),
            CycleLife::Million,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("bending-fatigue strength must lie below twice the tensile strength"),
            "{err}"
        );
    }
}
```

(`draft_from_material` — the same Task-1 helper convention; `Length::from_inches` and `Moment::from_pound_force_inches` are existing constructors.)

- [ ] **Step 2: Run to verify fail** — `cargo test -p springcore --lib torsion::fatigue` → COMPILE FAIL (no module items).

- [ ] **Step 3: Implement** (prepend above the tests):

```rust
//! Fatigue analysis for helical torsion springs (Shigley §10-12): the wire cycles
//! in BENDING, so the compression module's Sines/Zimmerli shear data does not
//! apply. Uses the Associated Spring R = 0 repeated-bending strengths (Table
//! 10-10, stored per material as fractions of Sut) with the GERBER criterion the
//! source prescribes: Se from Eq. 10-58, strength amplitude Sa from Eq. 10-59
//! along the load line r = Ma/Mm, and nf = Sa/σa (Eq. 10-60).

use crate::material::Material;
use crate::torsion::design::validate_wire_mean_geometry;
use crate::torsion::mechanics::bending_stress_inner;
use crate::units::{Length, Moment, Stress};
use crate::{Result, SpringError};

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

/// Torsion-spring fatigue analysis result (Shigley §10-12, Gerber).
#[derive(Debug, Clone, Copy)]
pub struct TorFatigueResult {
    /// σa = K_bi·32·Ma/(π·d³) (Eq. 10-44 at the alternating moment).
    pub alternating_stress: Stress,
    /// σm at the mean moment.
    pub mean_stress: Stress,
    /// Fully-reversed endurance Se (Eq. 10-58, the Gerber R = 0 conversion of Sr).
    pub fully_reversed_endurance: Stress,
    /// Sut(d) — the Gerber ultimate (bending: TENSILE, unlike compression's shear).
    pub ultimate_tensile: Stress,
    /// Gerber strength amplitude Sa (Eq. 10-59, load line r = Ma/Mm).
    pub strength_amplitude: Stress,
    /// nf = Sa/σa (Eq. 10-60).
    pub gerber_factor_of_safety: f64,
}

/// Analyze fatigue for a torsion spring cycling between `moment_min` and
/// `moment_max` (both winding the coil tighter — the R = 0 data's domain).
pub fn analyze_torsion_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    moment_min: Moment,
    moment_max: Moment,
    life: CycleLife,
) -> Result<TorFatigueResult> {
    // 1. Geometry first (error precedence; solve_forward's exact messages).
    validate_wire_mean_geometry(wire_dia, mean_dia)?;
    // 2. Data presence (compression's degradation path).
    let bf = material
        .bending_fatigue
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;
    // 3–5. The moment pair. Non-negative + finite (R = 0 domain), ordered, and
    // strictly differing: Gerber's nf = Sa/σa divides by σa (Eq. 10-60), so a zero
    // alternating moment must be a named error, not an ∞/NaN — unlike compression's
    // reciprocal Goodman form, which tolerates τa = 0.
    let (lo, hi) = (moment_min.newton_meters(), moment_max.newton_meters());
    if !(lo.is_finite() && lo >= 0.0 && hi.is_finite() && hi >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "cycle moments must be finite and non-negative \
             (the R = 0 bending data covers unidirectional winding loads)"
                .into(),
        ));
    }
    if hi < lo {
        return Err(SpringError::InconsistentInputs(
            "max cycle moment must be at least the min cycle moment".into(),
        ));
    }
    if hi == lo {
        return Err(SpringError::InconsistentInputs(
            "cycle moments must differ (a zero alternating moment has no fatigue amplitude)"
                .into(),
        ));
    }

    let ma = Moment::from_newton_meters((hi - lo) / 2.0);
    let mm = Moment::from_newton_meters((hi + lo) / 2.0);
    // σ via the cited inner-fiber helper (Ki = Eq. 10-43 = kbi_factor; the source
    // prescribes Ki — no selectable correction in bending).
    let sigma_a = bending_stress_inner(ma, mean_dia, wire_dia);
    let sigma_m = bending_stress_inner(mm, mean_dia, wire_dia);

    let sut = material.min_tensile_strength(wire_dia)?;
    let sut_pa = sut.pascals();
    let pct = match life {
        CycleLife::HundredThousand => bf.sr_pct_1e5,
        CycleLife::Million => bf.sr_pct_1e6,
    };
    let sr = pct * sut_pa;
    // 6. Eq. 10-58's denominator 1 − (Sr/2/Sut)² is ≤ 0 iff Sr ≥ 2·Sut — impossible
    // for Table 10-10 fractions (≤ 0.64) but REACHABLE through user-overlay
    // materials with absurd fractions; a silent negative/∞ Se would poison nf.
    if sr / 2.0 >= sut_pa {
        return Err(SpringError::InconsistentInputs(
            "bending-fatigue strength must lie below twice the tensile strength \
             (Eq. 10-58's denominator would be non-positive)"
                .into(),
        ));
    }
    let se = (sr / 2.0) / (1.0 - (sr / (2.0 * sut_pa)).powi(2));
    // Load-line slope r = Ma/Mm (Mm > 0: guard 5 excluded the both-zero pair).
    let r = ma.newton_meters() / mm.newton_meters();
    // Eq. 10-59: Sa = (r²·Sut²)/(2·Se) · (−1 + √(1 + (2·Se/(r·Sut))²)).
    let sa = (r * r * sut_pa * sut_pa) / (2.0 * se)
        * (-1.0 + (1.0 + (2.0 * se / (r * sut_pa)).powi(2)).sqrt());
    let nf = sa / sigma_a.pascals();

    Ok(TorFatigueResult {
        alternating_stress: sigma_a,
        mean_stress: sigma_m,
        fully_reversed_endurance: Stress::from_pascals(se),
        ultimate_tensile: sut,
        strength_amplitude: Stress::from_pascals(sa),
        gerber_factor_of_safety: nf,
    })
}
```

`torsion/mod.rs`: add `mod fatigue;` and `pub use fatigue::{analyze_torsion_fatigue, CycleLife, TorFatigueResult};`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib torsion::fatigue` → PASS (7 tests); full `cargo test -p springcore --lib` green.

- [ ] **Step 5: Full gate + commit**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Kill map: guard mutants → the pinned-message matrix (incl. >= vs > at hi==lo via
# the equal-moments cases); pct-match column swap → hundred_thousand monotonicity +
# chrome_vanadium exact-Se; Eq. 10-58/10-59 algebra → the golden (5e-3) + the
# 1e-12 self-consistency + chrome_vanadium's independent Se recomputation; the trap
# `>=` boundary (sr/2 == sut) is dyadically constructible if needed (fraction 2.0).
git add springcore/src/torsion/fatigue.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): Gerber fatigue analysis — Table 10-10 data, Example 10-8(c) golden"
```

- [ ] **Step 6: Final whole-branch review** — the controller dispatches the panel (general-code, architect, simplifier, MANDATORY input-domain adversary with numerical attention on the Gerber algebra — Eq. 10-59's cancellation for small r, the √ domain; persistence/wire-format reviewer for the materials raw-record/overlay surface), cycles to convergence, then pushes and opens the PR.

---

## Notes for the implementer

- **Never push, never create/edit PRs, never run review panels** — controller-only.
- The `draft_from_material` helper: whatever Material→Draft convention material.rs's
  tests already use — do NOT invent a new public API for it; if none exists, build
  the draft literally from the bundled material's fields inside the test module.
- The golden's 5e-3 tolerance absorbs the textbook's rounded intermediates (Ki
  printed as 1.115, Sut as 294.4 kpsi); the 1e-12 self-consistency asserts are the
  tight algebra pins. If a mutant survives the golden, strengthen via the
  self-consistency or the chrome_vanadium independent recomputation — never widen
  tolerances.
- `sigma_m` comes from `bending_stress_inner(mm, …)` directly (not σa·Mm/Ma) — same
  value, single code path, no ratio-formula duplication.
