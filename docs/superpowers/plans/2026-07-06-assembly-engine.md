# Assembly Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `springcore` family — nested/series assemblies of 1..N compression springs composed from the existing cited engine — plus additive persistence (the format's first nested-struct list) and the springmaker placeholder with the rejection signal reintroduced.

**Architecture:** `springcore/src/assembly/` holds only the COMBINATION layer: two passes over `crate::design::solve_forward` (pass 1 validates + yields rates; pass 2 solves each member at its topology-derived load share), then derived combination outputs with in-code derivations. Persistence adds `DesignSpec::Assembly(AssemblySpec)` with `Vec<AssemblyMemberSpec>`.

**Tech Stack:** Rust workspace — springcore (mutation-gated) + one springmaker placeholder arm.

## Global Constraints

- springcore mutation-gated: `cargo mutants --in-diff` vs origin/main ends `0 missed`. Strict TDD. Every message in this plan VERBATIM.
- NO references to the commercial inspiration product/vendor (tooling trailers exempt).
- MSRV 1.88; fmt zero deviation; clippy `-D warnings` clean.
- Commit DIRECTLY on `feat/assembly-engine` — NOT a side branch (a prior implementer strayed onto one; verify `git branch --show-current` before your first commit and state it in your report). NEVER push/PR/panel/marker; NEVER touch `.git/`.
- Conventional commits; trailer: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- Do NOT add `Family::Assembly` (GUI increment's job). Spec: docs/superpowers/specs/2026-07-06-assembly-engine-design.md (Decisions 1–7 bind; Decision-6 omissions are deliberate).

---

### Task 1: `springcore/src/assembly/` — the composition engine

**Files:**
- Create: `springcore/src/assembly/mod.rs`, `springcore/src/assembly/design.rs`
- Modify: `springcore/src/lib.rs` (`pub mod assembly;` in sorted position; re-exports)

**Interfaces:**
- Consumes: `crate::design::{solve_forward, index_caution_labeled, DesignStatus, StatusMessage, Severity, SpringDesign}`, `crate::material::MaterialSet` (`get(&self, name: &str) -> Result<&Material>`), `crate::end_type::EndType`, `crate::mechanics::{EndFixity, CurvatureCorrection… (CurvatureCorrection re-exported at crate root)}`, units.
- Produces (Task 2 + GUI rely on exact names): `Topology::{Nested, Series}`, `AssemblyMember`, `AssemblyInputs`, `MemberResult`, `AssemblyLoadPoint`, `AssemblyDesign`, `solve_assembly(materials: &MaterialSet, inputs: &AssemblyInputs, loads: &[Force], fixity: EndFixity, correction: CurvatureCorrection) -> Result<AssemblyDesign>`, `evaluate_status(design: &AssemblyDesign, materials: &MaterialSet) -> DesignStatus`.

- [ ] **Step 1: module scaffolding + failing tests**

`springcore/src/assembly/mod.rs`:

```rust
//! Assemblies of cylindrical round-wire compression springs — Nested
//! (concentric, parallel-acting) or Series (stacked).
//!
//! PURE COMPOSITION: every member is solved by the existing cited
//! compression engine (`crate::design::solve_forward`); this module adds
//! only the combination layer. Rate/load-sharing sources: Shigley 10th ed.
//! Ch. 4's worked nested-pair example (k = Σkᵢ, Fᵢ = kᵢF/Σk) and
//! Eq. 8-15 / Prob. 4-1 (1/k = Σ 1/kᵢ for series), each generalized to N
//! members by the same equilibrium argument (derivation notes at the
//! formula sites). §10-1 endorses nested round-wire springs explicitly.
//!
//! HONEST BOUNDARY: nested members must share a free length — staged
//! engagement (members engaging at different deflections) is
//! progressive-contact physics with no in-house citation, the same class
//! excluded for variable pitch and conical's post-bottoming regime.
//!
//! Deliberate omissions (none fabricated): opposite-hand winding
//! convention for adjacent nested members (industry practice, not in
//! Shigley); stack-level buckling for series (per-member stability flags
//! still surface, member-indexed); assembly-level surge frequency.

mod design;

pub use design::{
    evaluate_status, solve_assembly, AssemblyDesign, AssemblyInputs, AssemblyLoadPoint,
    AssemblyMember, MemberResult, Topology,
};
```

Add `pub mod assembly;` to `springcore/src/lib.rs` (alphabetically first, before `conical`).

In `springcore/src/assembly/design.rs`, write the TEST MODULE first (red = compile fail), then implement (Step 3). Tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::end_type::EndType;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    /// The compression baseline member: d=2mm, D=20mm, Na=10, L0=60mm,
    /// SquaredGround → k = 2000 N/m, Ls = 24mm (design.rs's own fixture).
    fn baseline_member() -> AssemblyMember {
        AssemblyMember {
            material_name: "Music Wire".to_string(),
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active_coils: 10.0,
            free_length: Length::from_millimeters(60.0),
            end_type: EndType::SquaredGround,
        }
    }

    /// A softer second member: D=25mm (k ≈ 1024 N/m), same wire/coils/L0.
    fn soft_member() -> AssemblyMember {
        AssemblyMember {
            mean_dia: Length::from_millimeters(25.0),
            ..baseline_member()
        }
    }

    fn materials() -> crate::MaterialSet {
        crate::MaterialSet::load_default()
    }

    fn solve(topology: Topology, members: Vec<AssemblyMember>, loads: &[f64]) -> crate::Result<AssemblyDesign> {
        let loads: Vec<Force> = loads.iter().map(|&n| Force::from_newtons(n)).collect();
        solve_assembly(
            &materials(),
            &AssemblyInputs { topology, members },
            &loads,
            EndFixity::FixedFixed,
            crate::CurvatureCorrection::Bergstrasser,
        )
    }

    // ── Identity oracles: one member == the bare compression solve ─────────

    #[test]
    fn one_member_assembly_matches_bare_solve_both_topologies() {
        let mats = materials();
        let material = mats.get("Music Wire").unwrap();
        let m = baseline_member();
        let bare = crate::design::solve_forward(
            material,
            m.end_type,
            EndFixity::FixedFixed,
            m.wire_dia,
            m.mean_dia,
            m.active_coils,
            m.free_length,
            &[Force::from_newtons(10.0)],
            crate::CurvatureCorrection::Bergstrasser,
        )
        .unwrap();
        for topology in [Topology::Nested, Topology::Series] {
            let asm = solve(topology, vec![baseline_member()], &[10.0]).unwrap();
            assert_relative_eq!(
                asm.rate.newtons_per_meter(),
                bare.rate.newtons_per_meter(),
                max_relative = 1e-12
            );
            assert_relative_eq!(
                asm.members[0].design.load_points[0].shear_stress.pascals(),
                bare.load_points[0].shear_stress.pascals(),
                max_relative = 1e-12
            );
            assert_relative_eq!(
                asm.solid_length.meters(),
                bare.solid_length.meters(),
                max_relative = 1e-12
            );
            assert_relative_eq!(asm.members[0].share_fraction, 1.0, max_relative = 1e-12);
            // Travel limit = the member's own: L0 − Ls = 36 mm; F = k·y = 72 N.
            assert_relative_eq!(
                asm.travel_limit_deflection.millimeters(),
                36.0,
                max_relative = 1e-9
            );
            assert_relative_eq!(asm.travel_limit_force.newtons(), 72.0, max_relative = 1e-9);
            assert_eq!(asm.limiting_member, 0);
        }
    }

    // ── Two identical members: closed forms ────────────────────────────────

    #[test]
    fn two_identical_nested_doubles_the_rate() {
        let asm = solve(
            Topology::Nested,
            vec![baseline_member(), baseline_member()],
            &[40.0],
        )
        .unwrap();
        assert_relative_eq!(asm.rate.newtons_per_meter(), 4000.0, max_relative = 1e-9);
        // Each member carries half the load (Ch. 4 rate-fraction result).
        for mr in &asm.members {
            assert_relative_eq!(mr.share_fraction, 0.5, max_relative = 1e-12);
            assert_relative_eq!(
                mr.design.load_points[0].force.newtons(),
                20.0,
                max_relative = 1e-9
            );
        }
        // Assembly deflection = F/k = 40/4000 = 10 mm; equals each member's.
        assert_relative_eq!(asm.load_points[0].deflection.millimeters(), 10.0, max_relative = 1e-9);
        assert_relative_eq!(
            asm.members[0].design.load_points[0].deflection.millimeters(),
            10.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn two_identical_series_halves_the_rate_and_sums_deflections() {
        let asm = solve(
            Topology::Series,
            vec![baseline_member(), baseline_member()],
            &[20.0],
        )
        .unwrap();
        assert_relative_eq!(asm.rate.newtons_per_meter(), 1000.0, max_relative = 1e-9);
        // Full force through each member.
        for mr in &asm.members {
            assert_relative_eq!(mr.share_fraction, 1.0, max_relative = 1e-12);
            assert_relative_eq!(
                mr.design.load_points[0].force.newtons(),
                20.0,
                max_relative = 1e-9
            );
        }
        // Assembly deflection = 20/1000 = 20 mm = 2 × each member's 10 mm.
        assert_relative_eq!(asm.load_points[0].deflection.millimeters(), 20.0, max_relative = 1e-9);
        // Free/solid lengths sum: 120 mm / 48 mm.
        assert_relative_eq!(asm.free_length.millimeters(), 120.0, max_relative = 1e-9);
        assert_relative_eq!(asm.solid_length.millimeters(), 48.0, max_relative = 1e-9);
    }

    // ── Ch. 4 shares with unequal members ──────────────────────────────────

    #[test]
    fn nested_shares_are_rate_fractions_and_deflections_equal() {
        let asm = solve(
            Topology::Nested,
            vec![baseline_member(), soft_member()],
            &[30.0],
        )
        .unwrap();
        let k0 = asm.members[0].design.rate.newtons_per_meter();
        let k1 = asm.members[1].design.rate.newtons_per_meter();
        assert_relative_eq!(asm.rate.newtons_per_meter(), k0 + k1, max_relative = 1e-12);
        assert_relative_eq!(asm.members[0].share_fraction, k0 / (k0 + k1), max_relative = 1e-12);
        assert_relative_eq!(asm.members[1].share_fraction, k1 / (k0 + k1), max_relative = 1e-12);
        // Fᵢ = kᵢF/Σk and every deflection equals the assembly's (Ch. 4).
        let y = asm.load_points[0].deflection.meters();
        for mr in &asm.members {
            assert_relative_eq!(
                mr.design.load_points[0].force.newtons(),
                mr.share_fraction * 30.0,
                max_relative = 1e-12
            );
            assert_relative_eq!(
                mr.design.load_points[0].deflection.meters(),
                y,
                max_relative = 1e-9
            );
        }
    }

    // ── Travel limits ───────────────────────────────────────────────────────

    #[test]
    fn series_limiting_member_is_the_first_to_bottom() {
        // Baseline (travel 36mm, k=2000 → bottoms at 72 N) vs a stiffer
        // short-travel member: L0=30mm → travel = 30−24 = 6mm, bottoms at
        // 2000·0.006 = 12 N. The short one limits.
        let short = AssemblyMember {
            free_length: Length::from_millimeters(30.0),
            ..baseline_member()
        };
        let asm = solve(Topology::Series, vec![baseline_member(), short], &[5.0]).unwrap();
        assert_eq!(asm.limiting_member, 1);
        assert_relative_eq!(asm.travel_limit_force.newtons(), 12.0, max_relative = 1e-9);
        // Assembly travel at that force: F/k_asm with k_asm = 1000 N/m → 12 mm.
        assert_relative_eq!(
            asm.travel_limit_deflection.millimeters(),
            12.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn nested_travel_limit_is_free_length_minus_max_solid() {
        // Equal L0 (required); make member 2 have more coils → larger Ls.
        // Na=14 → Nt=16 → Ls = 32 mm; travel = 60−32 = 28 mm; it limits.
        let chunky = AssemblyMember {
            active_coils: 14.0,
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![baseline_member(), chunky], &[10.0]).unwrap();
        assert_eq!(asm.limiting_member, 1);
        assert_relative_eq!(
            asm.travel_limit_deflection.millimeters(),
            28.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            asm.travel_limit_force.newtons(),
            asm.rate.newtons_per_meter() * 0.028,
            max_relative = 1e-9
        );
    }

    // ── Status: travel-limit boundary, clearance, member prefixes ──────────

    fn has_message(status: &crate::design::DesignStatus, needle: &str) -> bool {
        status.messages.iter().any(|m| m.message.contains(needle))
    }

    #[test]
    fn travel_limit_warning_boundary() {
        // Nested baseline pair: travel 36 mm at k=4000 → 144 N exactly at.
        let at = solve(
            Topology::Nested,
            vec![baseline_member(), baseline_member()],
            &[144.0],
        )
        .unwrap();
        let status = evaluate_status(&at, &materials());
        assert!(
            !has_message(&status, "exceeds the travel limit"),
            "exactly at the limit must not warn (strict >)"
        );
        let over = solve(
            Topology::Nested,
            vec![baseline_member(), baseline_member()],
            &[144.1],
        )
        .unwrap();
        let status = evaluate_status(&over, &materials());
        assert!(has_message(&status, "load point 1 exceeds the travel limit"));
        assert!(has_message(&status, "member 1 bottoms first"));
    }

    #[test]
    fn nested_clearance_boundary_and_series_never_warns() {
        // Outer = baseline (D=20 → ID = 18 mm). Inner exactly-at: OD = 18
        // → D_inner = 16 mm. Just clear: D_inner = 15.9 → OD = 17.9.
        let inner_at = AssemblyMember {
            mean_dia: Length::from_millimeters(16.0),
            ..baseline_member()
        };
        let asm = solve(
            Topology::Nested,
            vec![baseline_member(), inner_at.clone()],
            &[10.0],
        )
        .unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            has_message(&status, "nested interference"),
            "exactly-at is interference (≥)"
        );
        let inner_clear = AssemblyMember {
            mean_dia: Length::from_millimeters(15.9),
            ..baseline_member()
        };
        let asm = solve(
            Topology::Nested,
            vec![baseline_member(), inner_clear],
            &[10.0],
        )
        .unwrap();
        assert!(!has_message(&evaluate_status(&asm, &materials()), "nested interference"));
        // Series: identical geometry is legal, no clearance semantics.
        let asm = solve(
            Topology::Series,
            vec![baseline_member(), inner_at],
            &[10.0],
        )
        .unwrap();
        assert!(!has_message(&evaluate_status(&asm, &materials()), "nested interference"));
    }

    #[test]
    fn three_member_clearance_checks_adjacent_pairs_only() {
        // Sorted by mean dia: 12 / 16 / 20 mm — adjacent gaps clear
        // (OD 14 < ID 14?? — pick real numbers: D=12 → OD 14, next ID:
        // D=16 → ID 14 → 14 ≥ 14 interferes! Use 12/17/24:
        // OD(12)=14 < ID(17)=15 ✓clear; OD(17)=19 < ID(24)=22 ✓clear.
        let m = |d_mm: f64| AssemblyMember {
            mean_dia: Length::from_millimeters(d_mm),
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![m(24.0), m(12.0), m(17.0)], &[10.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            !has_message(&status, "nested interference"),
            "adjacent pairs clear regardless of member order in the input"
        );
    }

    #[test]
    fn member_statuses_carry_member_prefixes() {
        // Overstress member 2 only: tiny hard-driven spring as member 2.
        let small = AssemblyMember {
            wire_dia: Length::from_millimeters(1.0),
            mean_dia: Length::from_millimeters(8.0),
            active_coils: 6.0,
            ..baseline_member()
        };
        // Series so both see the full 40 N (overstresses the small one).
        let asm = solve(Topology::Series, vec![baseline_member(), small], &[40.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(has_message(&status, "member 2: load point 1 stress"));
        assert!(!has_message(&status, "member 1: load point 1 stress"));
    }

    // ── Guard matrix ────────────────────────────────────────────────────────

    fn msg(result: crate::Result<AssemblyDesign>) -> String {
        match result {
            Err(crate::SpringError::InconsistentInputs(m)) => m,
            other => panic!("expected InconsistentInputs, got {other:?}"),
        }
    }

    #[test]
    fn guards_pin_messages() {
        assert_eq!(
            msg(solve(Topology::Nested, vec![], &[10.0])),
            "an assembly needs at least one member"
        );
        assert_eq!(
            msg(solve(Topology::Nested, vec![baseline_member()], &[-1.0])),
            "loads must be finite and non-negative"
        );
        let long = AssemblyMember {
            free_length: Length::from_millimeters(70.0),
            ..baseline_member()
        };
        assert_eq!(
            msg(solve(Topology::Nested, vec![baseline_member(), long], &[10.0])),
            "nested members must share a free length (staged engagement is not modeled)"
        );
        // SERIES members may differ in free length — accepted.
        let long = AssemblyMember {
            free_length: Length::from_millimeters(70.0),
            ..baseline_member()
        };
        assert!(solve(Topology::Series, vec![baseline_member(), long], &[10.0]).is_ok());
    }

    #[test]
    fn member_errors_carry_the_member_prefix() {
        // Member 2 has a bad geometry (mean == wire).
        let bad = AssemblyMember {
            mean_dia: Length::from_millimeters(2.0),
            ..baseline_member()
        };
        let m = msg(solve(Topology::Series, vec![baseline_member(), bad], &[10.0]));
        assert_eq!(
            m,
            "member 2: mean diameter must be greater than wire diameter (spring index must exceed 1)"
        );
        // Unknown material on member 1.
        let ghost = AssemblyMember {
            material_name: "Unobtainium".to_string(),
            ..baseline_member()
        };
        let m = msg(solve(Topology::Series, vec![ghost], &[10.0]));
        assert_eq!(m, "member 1: material not found: Unobtainium");
        // DiameterOutOfRange re-emission caveat: a 10 mm music-wire member.
        let fat = AssemblyMember {
            wire_dia: Length::from_millimeters(10.0),
            mean_dia: Length::from_millimeters(80.0),
            free_length: Length::from_millimeters(200.0),
            ..baseline_member()
        };
        let m = msg(solve(Topology::Series, vec![fat], &[10.0]));
        assert!(
            m.starts_with("member 1: wire diameter") && m.contains("outside the valid range"),
            "got: {m}"
        );
    }

    #[test]
    fn top_level_material_is_not_consulted() {
        // Decision-2 semantic lives at the persistence/GUI layer; at the
        // engine layer the ONLY material inputs are member names. This test
        // pins that two members with different materials both resolve.
        let stainless = AssemblyMember {
            material_name: "Stainless 302".to_string(),
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![baseline_member(), stainless], &[10.0]).unwrap();
        assert_eq!(asm.members[0].material_name, "Music Wire");
        assert_eq!(asm.members[1].material_name, "Stainless 302");
    }

    #[test]
    fn huge_load_trips_the_output_guard() {
        let m = msg(solve(
            Topology::Nested,
            vec![baseline_member()],
            &[1e305],
        ));
        assert_eq!(
            m,
            "assembly solve produced a non-finite result (inputs exceed the representable range)"
        );
    }
}
```

(Adjust the three-member clearance fixture numbers if the in-test comment's arithmetic drifts — the CONTRACT is: three members, input order shuffled, all adjacent gaps clear → no warning. Verify `MaterialSet::load_default()` is the hermetic constructor sibling engine tests use — mirror their idiom if it differs. The Stainless 302 member relies on that material existing in the default set — verify; substitute any second bundled material.)

- [ ] **Step 2: red run**

Run: `cargo test -p springcore assembly`
Expected: compile FAIL (types not defined).

- [ ] **Step 3: implement `design.rs`**

Above the test module:

```rust
//! Assembly solve: two-pass composition over the compression engine plus
//! derived combination outputs. Sources at each site; module docs cover the
//! model boundary and omissions.

use crate::design::{index_caution_labeled, DesignStatus, Severity, SpringDesign, StatusMessage};
use crate::end_type::EndType;
use crate::material::MaterialSet;
use crate::mechanics::EndFixity;
use crate::units::{Force, Length, SpringRate};
use crate::{CurvatureCorrection, Result, SpringError};

/// Assembly topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topology {
    /// Concentric (parallel-acting) members: equal deflections, load shared
    /// by rate fraction (Shigley Ch. 4's nested-pair result), k = Σkᵢ.
    Nested,
    /// Stacked members: equal force through each, deflections sum,
    /// 1/k = Σ 1/kᵢ (Shigley Eq. 8-15, citing Prob. 4-1).
    Series,
}

/// One member's definition (geometry + its own material — Decision 2).
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
    /// Solved at THIS member's load share (Nested: rate-fraction forces;
    /// Series: the full assembly forces).
    pub design: SpringDesign,
    /// kᵢ/Σk — the Ch. 4 share. Series members all carry 1.0.
    pub share_fraction: f64,
}

/// Assembly-level state at one load (per-member detail lives in
/// `members[i].design.load_points`).
#[derive(Debug, Clone, Copy)]
pub struct AssemblyLoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
}

/// A solved assembly (linear composition of linear members).
#[derive(Debug, Clone)]
pub struct AssemblyDesign {
    pub topology: Topology,
    pub members: Vec<MemberResult>,
    /// Σkᵢ (Nested) or 1/Σ(1/kᵢ) (Series).
    pub rate: SpringRate,
    /// Nested: the shared member free length. Series: Σ free lengths.
    pub free_length: Length,
    /// Nested: max member solid length. Series: Σ member solid lengths.
    pub solid_length: Length,
    /// Usable travel before the first member bottoms (derived-geometric):
    /// Nested — deflection L₀ − max(Lsᵢ), all members deflect together;
    /// Series — set by the member with the least kᵢ·(L₀ᵢ − Lsᵢ).
    pub travel_limit_deflection: Length,
    pub travel_limit_force: Force,
    /// Index into `members` of the first member to bottom (ties: lowest).
    pub limiting_member: usize,
    pub load_points: Vec<AssemblyLoadPoint>,
}

/// Wrap a member-level error with its 1-based member attribution.
/// `InconsistentInputs` keeps its inner message (avoiding a doubled
/// "inconsistent inputs:" prefix); every other variant is flattened through
/// its `Display`. CAVEAT (spec §A): `DiameterOutOfRange` loses the GUI's
/// unit-aware re-formatting — member attribution beats unit localization
/// for v1; recorded for the GUI increment.
fn member_error(index: usize, err: SpringError) -> SpringError {
    let inner = match err {
        SpringError::InconsistentInputs(m) => m,
        other => other.to_string(),
    };
    SpringError::InconsistentInputs(format!("member {}: {inner}", index + 1))
}

pub fn solve_assembly(
    materials: &MaterialSet,
    inputs: &AssemblyInputs,
    loads: &[Force],
    fixity: EndFixity,
    correction: CurvatureCorrection,
) -> Result<AssemblyDesign> {
    if inputs.members.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "an assembly needs at least one member".into(),
        ));
    }
    // Assembly-level load domain (compression's exact message); pass-2
    // member solves see derived shares of these.
    if loads
        .iter()
        .any(|f| !f.newtons().is_finite() || f.newtons() < 0.0)
    {
        return Err(SpringError::InconsistentInputs(
            "loads must be finite and non-negative".into(),
        ));
    }
    // Nested members act in parallel from zero deflection, which requires a
    // shared free length (Ch. 4's premise). Staged engagement is out of
    // scope (module docs).
    if inputs.topology == Topology::Nested {
        let l0 = inputs.members[0].free_length.meters();
        if inputs
            .members
            .iter()
            .any(|m| m.free_length.meters() != l0)
        {
            return Err(SpringError::InconsistentInputs(
                "nested members must share a free length (staged engagement is not modeled)"
                    .into(),
            ));
        }
    }

    // Pass 1 — validate every member through the full compression solve
    // (geometry, material range, free-vs-solid) and collect rates.
    let mut rates = Vec::with_capacity(inputs.members.len());
    for (i, m) in inputs.members.iter().enumerate() {
        let material = materials
            .get(&m.material_name)
            .map_err(|e| member_error(i, e))?;
        let d = crate::design::solve_forward(
            material,
            m.end_type,
            fixity,
            m.wire_dia,
            m.mean_dia,
            m.active_coils,
            m.free_length,
            &[],
            correction,
        )
        .map_err(|e| member_error(i, e))?;
        rates.push(d.rate.newtons_per_meter());
    }
    let k_total = match inputs.topology {
        // k = Σkᵢ (Ch. 4, generalized to N by the same equilibrium argument).
        Topology::Nested => rates.iter().sum::<f64>(),
        // 1/k = Σ 1/kᵢ (Eq. 8-15, generalized to N).
        Topology::Series => 1.0 / rates.iter().map(|k| 1.0 / k).sum::<f64>(),
    };

    // Pass 2 — solve each member at its share for the real per-member state.
    let mut members = Vec::with_capacity(inputs.members.len());
    for (i, m) in inputs.members.iter().enumerate() {
        let share = match inputs.topology {
            Topology::Nested => rates[i] / rates.iter().sum::<f64>(),
            Topology::Series => 1.0,
        };
        let member_loads: Vec<Force> = loads
            .iter()
            .map(|f| Force::from_newtons(f.newtons() * share))
            .collect();
        let material = materials
            .get(&m.material_name)
            .map_err(|e| member_error(i, e))?;
        let design = crate::design::solve_forward(
            material,
            m.end_type,
            fixity,
            m.wire_dia,
            m.mean_dia,
            m.active_coils,
            m.free_length,
            &member_loads,
            correction,
        )
        .map_err(|e| member_error(i, e))?;
        members.push(MemberResult {
            material_name: m.material_name.clone(),
            design,
            share_fraction: share,
        });
    }

    // Combined lengths.
    let (free_length, solid_length) = match inputs.topology {
        Topology::Nested => (
            inputs.members[0].free_length,
            Length::from_meters(
                members
                    .iter()
                    .map(|mr| mr.design.solid_length.meters())
                    .fold(f64::NEG_INFINITY, f64::max),
            ),
        ),
        Topology::Series => (
            Length::from_meters(inputs.members.iter().map(|m| m.free_length.meters()).sum()),
            Length::from_meters(
                members
                    .iter()
                    .map(|mr| mr.design.solid_length.meters())
                    .sum(),
            ),
        ),
    };

    // Travel limit: the first member to bottom (derivations in the field
    // docs). Ties resolve to the lowest index (strict comparison).
    let (limiting_member, travel_limit_force) = match inputs.topology {
        Topology::Nested => {
            // All members share the deflection; the largest Ls bottoms first.
            let mut idx = 0;
            for (i, mr) in members.iter().enumerate() {
                if mr.design.solid_length.meters() > members[idx].design.solid_length.meters() {
                    idx = i;
                }
            }
            let travel = free_length.meters() - members[idx].design.solid_length.meters();
            (idx, Force::from_newtons(k_total * travel))
        }
        Topology::Series => {
            // Every member sees F; member i bottoms at kᵢ·(L₀ᵢ − Lsᵢ).
            let mut idx = 0;
            let mut f_min = f64::INFINITY;
            for (i, mr) in members.iter().enumerate() {
                let f_i = mr.design.rate.newtons_per_meter()
                    * (mr.design.free_length.meters() - mr.design.solid_length.meters());
                if f_i < f_min {
                    f_min = f_i;
                    idx = i;
                }
            }
            (idx, Force::from_newtons(f_min))
        }
    };
    let travel_limit_deflection =
        Length::from_meters(travel_limit_force.newtons() / k_total);

    // Assembly-level load points: y = F/k, length = L0 − y.
    let load_points: Vec<AssemblyLoadPoint> = loads
        .iter()
        .map(|&f| {
            let y = f.newtons() / k_total;
            AssemblyLoadPoint {
                force: f,
                deflection: Length::from_meters(y),
                length: Length::from_meters(free_length.meters() - y),
            }
        })
        .collect();

    // Output-finiteness guard (the cross-family standard).
    if [
        k_total,
        travel_limit_deflection.meters(),
        travel_limit_force.newtons(),
    ]
    .into_iter()
    .chain(
        load_points
            .iter()
            .flat_map(|lp| [lp.deflection.meters(), lp.length.meters()]),
    )
    .any(|v| !v.is_finite())
    {
        return Err(SpringError::InconsistentInputs(
            "assembly solve produced a non-finite result (inputs exceed the representable \
             range)"
                .into(),
        ));
    }

    Ok(AssemblyDesign {
        topology: inputs.topology,
        members,
        rate: SpringRate::from_newtons_per_meter(k_total),
        free_length,
        solid_length,
        travel_limit_deflection,
        travel_limit_force,
        limiting_member,
        load_points,
    })
}

/// Engineering status checks for a solved assembly.
pub fn evaluate_status(design: &AssemblyDesign, materials: &MaterialSet) -> DesignStatus {
    let mut messages = Vec::new();

    // Nested clearance (geometric): with members ordered by mean diameter,
    // any interference implies an adjacent-pair interference, so checking
    // adjacent pairs is complete. Exactly-touching counts (≥).
    if design.topology == Topology::Nested {
        let mut order: Vec<usize> = (0..design.members.len()).collect();
        order.sort_by(|&a, &b| {
            design.members[a]
                .design
                .mean_dia
                .meters()
                .total_cmp(&design.members[b].design.mean_dia.meters())
        });
        for pair in order.windows(2) {
            let (inner, outer) = (pair[0], pair[1]);
            if design.members[inner].design.outer_dia.meters()
                >= design.members[outer].design.inner_dia.meters()
            {
                messages.push(StatusMessage {
                    severity: Severity::Warning,
                    message: format!(
                        "members {} and {}: nested interference — the inner member's outer \
                         diameter meets or exceeds the outer member's inner diameter",
                        inner + 1,
                        outer + 1
                    ),
                });
            }
        }
    }

    // Per-member engineering status, member-prefixed. The Err-skip on the
    // material lookup is unreachable in practice (pass 1 resolved the same
    // names) — the conical precedent.
    for (i, mr) in design.members.iter().enumerate() {
        if let Some(msg) =
            index_caution_labeled(&format!("member {} spring index", i + 1), mr.design.index)
        {
            messages.push(msg);
        }
        let Ok(material) = materials.get(&mr.material_name) else {
            continue;
        };
        let allowable = material.allowable_pct_torsion;
        for (j, lp) in mr.design.load_points.iter().enumerate() {
            if lp.pct_mts > allowable {
                messages.push(StatusMessage {
                    severity: Severity::Warning,
                    message: format!(
                        "member {}: load point {} stress is {:.1}% of MTS, above the \
                         allowable {:.0}%",
                        i + 1,
                        j + 1,
                        lp.pct_mts * 100.0,
                        allowable * 100.0
                    ),
                });
            }
        }
        if mr.design.at_solid.pct_mts > material.allowable_pct_set {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "member {}: stress at solid is {:.1}% of MTS, above the set allowable \
                     {:.0}%",
                    i + 1,
                    mr.design.at_solid.pct_mts * 100.0,
                    material.allowable_pct_set * 100.0
                ),
            });
        }
        if !mr.design.buckling_stable {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "member {}: free length exceeds the absolute-stability limit; buckling \
                     possible",
                    i + 1
                ),
            });
        }
    }

    // Travel-limit exceedance (strict >: exactly-at is the boundary state).
    for (j, lp) in design.load_points.iter().enumerate() {
        if lp.deflection.meters() > design.travel_limit_deflection.meters() {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {} exceeds the travel limit (member {} bottoms first)",
                    j + 1,
                    design.limiting_member + 1
                ),
            });
        }
    }

    DesignStatus { messages }
}
```

Add the re-exports to `springcore/src/lib.rs` mirroring the conical line: `pub use assembly::{...}` if siblings re-export at the root — CHECK how conical is re-exported at the root (it may be module-path-only); mirror exactly.

- [ ] **Step 4: green + workspace + commit**

Run: `cargo test -p springcore assembly` → PASS; then `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all && cargo fmt --all --check`.

```bash
git add springcore/src/assembly springcore/src/lib.rs
git commit -m "feat(assembly): nested/series composition engine over the compression solver

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 5: mutation gate**

```bash
git diff origin/main...HEAD > /tmp/assembly-t1.diff
cargo mutants --in-diff /tmp/assembly-t1.diff --package springcore
```
Expected `0 missed`; kill survivors with tests (the share/limit arithmetic and the two `sort/fold` reductions are mutant-rich — expect several rounds).

---

### Task 2: persistence + springmaker placeholder

**Files:**
- Modify: `springcore/src/persistence.rs` (AssemblySpec, AssemblyMemberSpec, parse_topology, parse_fixity → pub, DesignSpec variant, rejection arm, tests)
- Modify: `springcore/src/lib.rs` (export AssemblySpec, AssemblyMemberSpec, parse_topology, parse_fixity)
- Modify: `springmaker/src/app.rs` (apply_saved → bool + Assembly reject arm + real-path test)

**Interfaces:**
- Consumes: Task 1's types are NOT consumed (solve_with_material REJECTS assembly — sibling pattern). `parse_end_type`'s shape as the template.
- Produces: `AssemblySpec::PowerUser { topology: String, fixity: String, members: Vec<AssemblyMemberSpec>, loads_n: Vec<f64> }`, `AssemblyMemberSpec { material_name, end_type, wire_dia_mm, mean_dia_mm, active, free_length_mm }`, `pub fn parse_topology(&str) -> Result<Topology>`, `parse_fixity` promoted `pub`.

- [ ] **Step 1: failing persistence tests** (mirror the VALID_CONICAL_TOML anchor idiom at persistence.rs:~1835 — a `VALID_ASSEMBLY_TOML` const with ONE `[[design.members]]` block, Ok-anchored; verify the actual serialized layout by printing a round-tripped `to_toml()` first and match it):

```rust
    #[test]
    fn assembly_round_trips_one_and_three_members() { /* build SavedDesign with
        DesignSpec::Assembly(AssemblySpec::PowerUser{ topology: "nested".into(),
        fixity: "fixed_fixed".into(), members: vec![member_spec(); 1 or 3],
        loads_n: vec![10.0, 25.0] }), to_toml → from_toml → assert_eq — two
        cases in one test or two tests per the file's conventions */ }

    #[test]
    fn from_toml_rejects_missing_field_inside_a_member() {
        // .replace() a required member key out of VALID_ASSEMBLY_TOML
        // (rename "wire_dia_mm" → "wire_diam") → DataFile.
    }

    #[test]
    fn from_toml_rejects_non_finite_inside_a_member_and_in_loads() {
        // .replace() "2.0" → "inf" inside the member block → DataFile;
        // separately loads_n → [10.0, nan] → DataFile.
    }

    #[test]
    fn from_toml_rejects_unknown_topology() {
        // .replace() "nested" → "stacked" → DataFile("unknown topology: stacked")
        // NOTE: topology is a raw String in the spec struct — the reject
        // happens in parse_topology at SOLVE/GUI time, NOT at deserialize.
        // Pin parse_topology directly instead:
        assert!(matches!(
            parse_topology("stacked"),
            Err(SpringError::DataFile(m)) if m == "unknown topology: stacked"
        ));
        assert!(matches!(parse_topology("nested"), Ok(Topology::Nested)));
        assert!(matches!(parse_topology("series"), Ok(Topology::Series)));
    }

    #[test]
    fn solve_with_material_rejects_assembly_design() {
        // Sibling-pattern pin, message VERBATIM:
        // "SavedDesign::solve handles compression designs; assembly designs \
        //  are solved via the assembly scenario"
    }

    #[test]
    fn top_level_material_differs_from_members_and_still_parses() {
        // Decision-2 semantic: SavedDesign.material = "Chrome-Vanadium",
        // member material_name = "Music Wire" — round-trips losslessly; the
        // file-level material is NOT rewritten to match members.
    }
```

Write these with REAL bodies following the file's conventions (the sketches above define the contracts; the neighboring conical tests define the shapes — copy them). Red run: `cargo test -p springcore persistence` → compile FAIL.

- [ ] **Step 2: implement persistence**

- `AssemblySpec` + `AssemblyMemberSpec` per the spec §B block (internally tagged `#[serde(tag = "type")]` on AssemblySpec; the member struct is a plain derive — all fields required).
- `DesignSpec::Assembly(AssemblySpec)` after `Conical`.
- `pub fn parse_topology(s: &str) -> Result<Topology>` beside `parse_end_type` (match "nested"/"series", else `DataFile(format!("unknown topology: {s}"))`); promote `parse_fixity` (persistence.rs:279) to `pub` with a doc comment (the GUI increment needs it; same rationale as parse_end_type).
- The `solve_with_material` rejection arm after Conical's, message VERBATIM: `"SavedDesign::solve handles compression designs; assembly designs are solved via the assembly scenario"`.
- Exports in lib.rs beside the existing persistence exports.
- Doc comment at the `Assembly` variant recording the Decision-2 semantic (top-level material = active picker state; member materials govern the solve).

- [ ] **Step 3: the springmaker placeholder (apply_saved regains the signal)**

`springmaker/src/app.rs`: change `fn apply_saved(&mut self, saved: SavedDesign)` back to `-> bool`, with a doc comment that references the conical spec's reversal note — this is the mechanical reintroduction it anticipated:

```rust
    /// Apply a loaded design to the app. Returns `false` when the design's
    /// family has no GUI yet (nothing is applied and `action_error` is set)
    /// so `load_from` can skip the recompute that would wipe the error —
    /// the load-path invariant from the conical increment. (The conical GUI
    /// spec's Decision-5 reversal note anticipated this signal returning
    /// with the next placeholder; here it is.)
    fn apply_saved(&mut self, saved: SavedDesign) -> bool {
        if matches!(saved.design, springcore::DesignSpec::Assembly(_)) {
            self.action_error = Some(
                "assembly designs are not supported by this build yet (the assembly GUI \
                 ships in the next increment)"
                    .into(),
            );
            return false;
        }
        self.material = saved.material;
        ...existing body...; the match keeps an
        `springcore::DesignSpec::Assembly(_) => unreachable!("handled above")` arm
        for exhaustiveness (the conical placeholder's accepted shape);
        all real arms fall through to `true`.
    }
```

`load_from`'s Ok arm: `Ok(saved) => self.apply_saved(saved),`.

TDD: FIRST write the real-path test — drive `load_from` against an assembly TOML tempfile (read the current load tests for the tempfile idiom), then mirror `update`'s contract exactly: `if app.load_from(&path) { app.recompute(); }`. Assert: `load_from` returned false (so the recompute block did not run), `action_error` contains "assembly designs are not supported", and material/unit_system are unchanged from pre-seeded values. This is the conical-era real-path shape — the error survives BECAUSE the false return suppresses the recompute. The red run is the compile failure against the current `()`-returning `apply_saved`; implement, then green.

- [ ] **Step 4: full gate + commit**

```bash
cargo fmt --all && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
git diff origin/main...HEAD > /tmp/assembly-full.diff
cargo mutants --in-diff /tmp/assembly-full.diff --package springcore
```
All clean; `0 missed`.

```bash
git add springcore/src/persistence.rs springcore/src/lib.rs springmaker/src/app.rs
git commit -m "feat(assembly): additive AssemblySpec persistence + placeholder with the rejection signal restored

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
