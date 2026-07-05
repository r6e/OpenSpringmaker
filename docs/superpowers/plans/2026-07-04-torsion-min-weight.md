# Torsion Minimum-Weight Optimizer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `springcore::torsion::solve_min_weight` — the lightest torsion design meeting a required rate and a max-moment stress allowable, with a user-chosen mean-diameter policy.

**Architecture:** One new module `springcore/src/torsion/optimize.rs` (types + validation + the analytic per-candidate search) re-exported from `torsion/mod.rs`, structurally mirroring `extension/optimize.rs` but WITHOUT root-finding: torsion mass is D-independent at fixed rate+wire, stress feasibility is one evaluation at the interval ceiling (K_bi monotone decreasing), and Compact's stress-governed D comes from a closed-form quadratic.

**Tech Stack:** Rust (MSRV 1.88), approx (test asserts), cargo-mutants (in-diff gate).

## Global Constraints

- springcore mutation-gated to **literal 0 survivors**: `git diff origin/main -- > /tmp/pr.diff && cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features` after every task.
- Strict TDD. Engine-only (no springmaker/, no persistence). All formulas cited (Shigley Ch. 10 / EN 13906-3). No commercial/vendor names. No `#[allow(dead_code)]`, no `todo!()`.
- Error split: malformed request → `SpringError::InconsistentInputs`; well-formed but empty feasible set → `SpringError::Infeasible`.
- The analytic structure is the spec (docs/superpowers/specs/2026-07-04-torsion-min-weight-design.md): D-independent mass; one-evaluation stress feasibility at the ceiling; Compact via the K_bi quadratic `C²(4−4t) + C(4t−1) − 1 = 0`; **t ≤ 1 unreachable past the feasibility check — documented at the code, never branched**.
- Building blocks (exact, verified): `mechanics::{kbi_factor(c) -> f64, bending_stress_inner(moment, mean_dia, wire_dia) -> Stress, active_coils_for_rate(youngs_modulus, wire_dia, mean_dia, rate, friction) -> f64, active_coils_with_legs(body_coils, leg1, leg2, mean_dia) -> f64}`; `design::solve_forward(material, TorsionInputs, &[Moment], FrictionModel)`; `Material { youngs_modulus, density (.kg_per_m3()), allowable_pct_bending, min_tensile_strength(d) -> Result<Stress> }`.
- Golden oracle: Music Wire (E = 203.4 GPa), k′ = 0.5085 N·m/rad ↔ Nₐ = 5 at d = 2 mm, D = 20 mm (PureBending, denom 64; ShigleyFriction denom = TAU·10.8).

---

## File Structure

- Create `springcore/src/torsion/optimize.rs` — everything (types, validation, search, tests).
- Modify `springcore/src/torsion/mod.rs` — `mod optimize;` + `pub use optimize::{solve_min_weight, DiaPolicy, TorBindingConstraint, TorMinWeightRequest, TorMinWeightSolution};` (extension-family precedent).

---

### Task 1: Types, request validation, and the honest Infeasible skeleton

**Files:**
- Create: `springcore/src/torsion/optimize.rs`
- Modify: `springcore/src/torsion/mod.rs`

**Interfaces:**
- Consumes: `crate::material::Material`, `crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs}` (Task 2 uses them; import in Task 2), `crate::torsion::mechanics::FrictionModel`, `crate::units::{AngularRate, Length, Moment}`, `crate::{Result, SpringError}`.
- Produces (Task 2 relies on these EXACT shapes): `DiaPolicy { MaxMargin (#[default]), Compact }`; `TorBindingConstraint { BendingStress, Index, OuterDiameter }`; `TorMinWeightRequest { required_rate: AngularRate, max_moment: Moment, leg1: Length, leg2: Length, friction_model: FrictionModel, dia_policy: DiaPolicy, index_bounds: (f64, f64), max_outer_dia: Option<Length>, arbor_dia: Option<Length>, candidate_diameters: Vec<Length> }`; `TorMinWeightSolution { design: TorsionDesign, binding: TorBindingConstraint, mass_kg: f64 }`; `pub fn solve_min_weight(material: &Material, req: &TorMinWeightRequest) -> Result<TorMinWeightSolution>` — in THIS task it validates, then returns `Infeasible` (no candidate logic yet; Task 2 replaces the loop body).

- [ ] **Step 1: Write the failing tests**

Create `springcore/src/torsion/optimize.rs` with ONLY the test module first (it won't compile until Step 3 adds the items — that's the RED):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::music_wire;
    use crate::torsion::FrictionModel;
    use crate::units::{AngularRate, Length, Moment};

    fn base_request() -> TorMinWeightRequest {
        TorMinWeightRequest {
            required_rate: AngularRate::from_newton_meters_per_radian(0.5085),
            max_moment: Moment::from_newton_millimeters(100.0),
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            friction_model: FrictionModel::PureBending,
            dia_policy: DiaPolicy::MaxMargin,
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            arbor_dia: None,
            candidate_diameters: vec![
                Length::from_millimeters(1.5),
                Length::from_millimeters(2.0),
                Length::from_millimeters(2.5),
            ],
        }
    }

    #[test]
    fn rejects_non_positive_or_non_finite_rate() {
        let m = music_wire();
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let req = TorMinWeightRequest {
                required_rate: AngularRate::from_newton_meters_per_radian(bad),
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("required rate must be a positive finite number"),
                    "rate={bad}: {msg}"
                ),
                other => panic!("rate={bad} must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_non_positive_or_non_finite_max_moment() {
        let m = music_wire();
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let req = TorMinWeightRequest {
                max_moment: Moment::from_newton_millimeters(bad),
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("max moment must be a positive finite number"),
                    "moment={bad}: {msg}"
                ),
                other => panic!("moment={bad} must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_negative_or_non_finite_legs() {
        let m = music_wire();
        for (l1, l2) in [(-1.0, 0.0), (0.0, f64::NAN), (f64::INFINITY, 0.0)] {
            let req = TorMinWeightRequest {
                leg1: Length::from_millimeters(l1),
                leg2: Length::from_millimeters(l2),
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("leg lengths must be finite and non-negative"),
                    "legs=({l1},{l2}): {msg}"
                ),
                other => panic!("legs=({l1},{l2}) must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_bad_index_bounds() {
        // c_min must exceed 1 (K_bi's domain), be finite, and lie strictly below
        // c_max. NOTE: deliberately NO 2+sqrt(3) floor (see the validation comment).
        let m = music_wire();
        for (lo, hi) in [
            (1.0, 12.0),   // c_min == 1: K_bi undefined at 1, monotone only above
            (0.5, 12.0),   // below 1
            (6.0, 6.0),    // not strictly increasing
            (8.0, 4.0),    // inverted
            (f64::NAN, 12.0),
            (4.0, f64::INFINITY),
        ] {
            let req = TorMinWeightRequest {
                index_bounds: (lo, hi),
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("index bounds"),
                    "bounds=({lo},{hi}): {msg}"
                ),
                other => panic!("bounds=({lo},{hi}) must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_bad_optional_diameters_and_candidates() {
        let m = music_wire();
        // max_outer_dia / arbor_dia: must be positive finite when present.
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let req = TorMinWeightRequest {
                max_outer_dia: Some(Length::from_millimeters(bad)),
                ..base_request()
            };
            assert!(
                matches!(
                    solve_min_weight(&m, &req),
                    Err(crate::SpringError::InconsistentInputs(_))
                ),
                "max_outer_dia={bad} must be rejected"
            );
            let req = TorMinWeightRequest {
                arbor_dia: Some(Length::from_millimeters(bad)),
                ..base_request()
            };
            assert!(
                matches!(
                    solve_min_weight(&m, &req),
                    Err(crate::SpringError::InconsistentInputs(_))
                ),
                "arbor_dia={bad} must be rejected"
            );
        }
        // Candidates: non-empty, all positive finite.
        let req = TorMinWeightRequest {
            candidate_diameters: vec![],
            ..base_request()
        };
        match solve_min_weight(&m, &req) {
            Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                msg.contains("candidate_diameters must contain at least one diameter"),
                "{msg}"
            ),
            other => panic!("empty candidates must be rejected, got {other:?}"),
        }
        for bad in [0.0, -1.0, f64::NAN] {
            let req = TorMinWeightRequest {
                candidate_diameters: vec![Length::from_millimeters(bad)],
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => assert!(
                    msg.contains("candidate diameters must be finite and positive"),
                    "candidate={bad}: {msg}"
                ),
                other => panic!("candidate={bad} must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn well_formed_request_reaches_the_search() {
        // Task 1's skeleton has no candidate logic: a fully valid request must fall
        // through validation and land on Infeasible (NOT InconsistentInputs), with
        // the Infeasible message pinned (kills the format-string mutant). Task 2
        // DELETES this test — the golden oracle supersedes it.
        let m = music_wire();
        match solve_min_weight(&m, &base_request()) {
            Err(crate::SpringError::Infeasible(msg)) => assert!(
                msg.contains("no candidate wire diameter"),
                "expected the pinned Infeasible message; got: {msg}"
            ),
            other => panic!("valid request must reach the (empty) search, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p springcore --lib torsion::optimize`
Expected: COMPILE FAIL — `TorMinWeightRequest` etc. not found.

- [ ] **Step 3: Implement the types + validation + skeleton**

Prepend to `optimize.rs` (above the test module):

```rust
//! Minimum-weight torsion-spring optimizer. Sibling of `crate::optimize`
//! (compression) and `extension::optimize`, built on the phase-2 rate inversion.
//!
//! STRUCTURAL INSIGHT (drives the whole module — see the design spec): at fixed
//! rate k' and wire d, the body wire length π·D·N_b = π·E·d⁴/(denom·k') − (L₁+L₂)/3
//! is INDEPENDENT of the mean diameter D (both Nₐ and the leg term scale as 1/D),
//! so mass is a strictly increasing function of d alone. The search therefore
//! needs no root-finding: the lightest feasible candidate diameter wins, and D is
//! chosen by policy ([`DiaPolicy`]). Formulas: Shigley Ch. 10 (Eq. 10-43 K_bi,
//! Eq. 10-44 σᵢ, Eq. 10-50/10-51 rate); EN 13906-3.

use crate::material::Material;
use crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs};
use crate::torsion::mechanics::FrictionModel;
use crate::units::{AngularRate, Length, Moment};
use crate::{Result, SpringError};

/// How the winning candidate's mean diameter is chosen — torsion mass is
/// D-independent at fixed rate and wire (module doc), so D is policy, not
/// optimization.
#[non_exhaustive] // sibling parity (HookSpec precedent): variants may be added
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiaPolicy {
    /// Largest allowed D: minimum bending stress (K_bi falls with index), maximum
    /// margin (default).
    #[default]
    MaxMargin,
    /// Smallest D that satisfies the stress allowable: the most compact coil.
    Compact,
}

/// Which constraint bound the chosen design.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorBindingConstraint {
    /// σᵢ reached the bending allowable (Compact policy, stress-governed D).
    BendingStress,
    /// A spring-index bound set D (the c_max ceiling under MaxMargin; the c_min
    /// floor under Compact when stress is already satisfied there).
    Index,
    /// The outer-diameter cap set D (MaxMargin with OD − d < c_max·d).
    OuterDiameter,
}

/// A minimum-weight torsion-spring problem.
#[derive(Debug, Clone)]
pub struct TorMinWeightRequest {
    /// Required angular rate k′; fixes Nₐ per (d, D) via `active_coils_for_rate`.
    /// Must be finite and > 0.
    pub required_rate: AngularRate,
    /// Maximum applied moment; σᵢ is evaluated here and it becomes the design's
    /// single load point. Must be finite and > 0.
    pub max_moment: Moment,
    /// Straight-leg length L₁ (finite, ≥ 0): enters the body-coil derivation AND
    /// the wire mass — a contribution no sibling optimizer has.
    pub leg1: Length,
    /// Straight-leg length L₂ (finite, ≥ 0).
    pub leg2: Length,
    /// Rate model — changes the Nₐ denominator (64 vs 2π·10.8), hence the mass
    /// itself, not just the reported rate.
    pub friction_model: FrictionModel,
    /// Mean-diameter selection policy (see [`DiaPolicy`]).
    pub dia_policy: DiaPolicy,
    /// Allowed spring-index range (c_min, c_max): both finite, 1 < c_min < c_max.
    /// NOTE: deliberately NO `2 + √3` floor — that sibling floor exists for the
    /// extension/compression stress factors' turning points; torsion's K_bi is
    /// monotone decreasing for ALL C > 1, so C > 1 is the only monotonicity
    /// requirement.
    pub index_bounds: (f64, f64),
    /// Optional cap on the outer diameter D + d. Finite and > 0 when present.
    pub max_outer_dia: Option<Length>,
    /// Optional arbor passthrough: validated, handed to `solve_forward`, whose
    /// arbor advisories ride along in the returned design's status. NOT a hard
    /// optimizer constraint (advisory-only in the engine, kept advisory here).
    pub arbor_dia: Option<Length>,
    /// Wire diameters to search; the lightest feasible one wins. Non-empty, all
    /// finite and > 0.
    pub candidate_diameters: Vec<Length>,
}

/// The chosen design, why it is limited, and its wire mass.
#[derive(Debug, Clone)]
pub struct TorMinWeightSolution {
    pub design: TorsionDesign,
    pub binding: TorBindingConstraint,
    pub mass_kg: f64,
}

/// Validate the request up front: malformed inputs are `InconsistentInputs`, never
/// `Infeasible` (mirrors the sibling optimizers' contract) — a bad request must not
/// masquerade as an empty feasible set.
fn validate_request(req: &TorMinWeightRequest) -> Result<()> {
    let rate = req.required_rate.newton_meters_per_radian();
    if !(rate.is_finite() && rate > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "required rate must be a positive finite number (N·m/rad)".into(),
        ));
    }
    let moment = req.max_moment.newton_meters();
    if !(moment.is_finite() && moment > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "max moment must be a positive finite number".into(),
        ));
    }
    let (l1, l2) = (req.leg1.meters(), req.leg2.meters());
    if !(l1.is_finite() && l1 >= 0.0 && l2.is_finite() && l2 >= 0.0) {
        return Err(SpringError::InconsistentInputs(
            "leg lengths must be finite and non-negative".into(),
        ));
    }
    let (c_min, c_max) = req.index_bounds;
    // K_bi = (4C²−C−1)/(4C(C−1)) is defined and monotone DECREASING for all C > 1
    // (Shigley Eq. 10-43), so C > 1 is the only monotonicity precondition — the
    // sibling `2 + √3` floor (their stress factors' turning points) does not apply.
    if !(c_min.is_finite() && c_max.is_finite() && c_min > 1.0 && c_min < c_max) {
        return Err(SpringError::InconsistentInputs(format!(
            "index bounds must satisfy 1 < c_min < c_max with both finite; \
             got c_min={c_min}, c_max={c_max}"
        )));
    }
    if let Some(od) = req.max_outer_dia {
        let v = od.meters();
        if !(v.is_finite() && v > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "max outer diameter must be a positive finite number".into(),
            ));
        }
    }
    if let Some(a) = req.arbor_dia {
        let v = a.meters();
        if !(v.is_finite() && v > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "arbor diameter must be a positive finite number".into(),
            ));
        }
    }
    if req.candidate_diameters.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "candidate_diameters must contain at least one diameter".into(),
        ));
    }
    if req.candidate_diameters.iter().any(|d| {
        let m = d.meters();
        !(m.is_finite() && m > 0.0)
    }) {
        return Err(SpringError::InconsistentInputs(
            "candidate diameters must be finite and positive".into(),
        ));
    }
    Ok(())
}

/// Pick the lightest feasible torsion design over the candidate wire diameters.
/// See the module doc for the analytic structure; the design spec records the
/// derivation.
pub fn solve_min_weight(
    material: &Material,
    req: &TorMinWeightRequest,
) -> Result<TorMinWeightSolution> {
    validate_request(req)?;
    let best: Option<TorMinWeightSolution> = None;
    // Task 2 fills the per-candidate search here.
    best.ok_or_else(|| {
        SpringError::Infeasible(format!(
            "no candidate wire diameter (of {}) yields a feasible torsion design",
            req.candidate_diameters.len()
        ))
    })
}
```

Wire `springcore/src/torsion/mod.rs`: add `mod optimize;` after `mod mechanics;` and the re-export line
`pub use optimize::{solve_min_weight, DiaPolicy, TorBindingConstraint, TorMinWeightRequest, TorMinWeightSolution};`
after the mechanics `pub use`. (`solve_forward`/`TorsionInputs` imports become used in Task 2 — in Task 1 remove them from the `use` lines if clippy flags unused; Task 2 restores them. `TorsionDesign` IS used by the solution struct.)

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib torsion::optimize` → PASS (6 tests). Also `cargo test -p springcore --lib` all green.

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected 0 survivors: every guard has a message-asserting test; the skeleton's
# `best` is a literal None (no logic to mutate); the Infeasible message is pinned
# by well_formed_request_reaches_the_search's contains assertion.
git add springcore/src/torsion/optimize.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): min-weight optimizer types, validation, and search skeleton"
```

---

### Task 2: The analytic per-candidate search

**Files:**
- Modify: `springcore/src/torsion/optimize.rs` (replace the skeleton loop; add helpers + tests)

**Interfaces:**
- Consumes: Task 1's types EXACTLY; `mechanics::{kbi_factor, bending_stress_inner, active_coils_for_rate, active_coils_with_legs}`; `design::{solve_forward, TorsionInputs}`; `Material::{min_tensile_strength, allowable_pct_bending, density, youngs_modulus}`.
- Produces: the complete `solve_min_weight`; private helpers `fn compact_index_for_stress(t: f64) -> f64` and `fn wire_mass(material: &Material, wire_dia: Length, mean_dia: Length, body_coils: f64, leg1: Length, leg2: Length) -> f64`.

- [ ] **Step 1: Write the failing tests** (add to `mod tests`; `use approx::assert_relative_eq;` and `use std::f64::consts::{PI, TAU};` join the test imports)

```rust
    #[test]
    fn golden_oracle_smallest_feasible_candidate_wins_max_margin() {
        // Low moment → every candidate is stress-feasible → the smallest d wins
        // (mass strictly increasing in d). MaxMargin, no cap → D = c_max·d,
        // binding = Index. Mass checked against the closed form
        // ρ·(π/4)d²·(π·E·d⁴/(64·k′)) (no legs), reading ρ and E from the material
        // so the oracle is exact without hardcoding material constants.
        let m = music_wire();
        let sol = solve_min_weight(&m, &base_request()).expect("feasible");
        let d = 0.0015_f64;
        assert_relative_eq!(
            sol.design.inputs.wire_dia.meters(),
            d,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            sol.design.inputs.mean_dia.meters(),
            12.0 * d,
            max_relative = 1e-12
        );
        assert_eq!(sol.binding, TorBindingConstraint::Index);
        let e = m.youngs_modulus.pascals();
        let expected_len = PI * e * d.powi(4) / (64.0 * 0.5085);
        let expected_mass = m.density.kg_per_m3() * (PI / 4.0) * d * d * expected_len;
        assert_relative_eq!(sol.mass_kg, expected_mass, max_relative = 1e-9);
        // The design solves at the requested rate (round-trip through the engine).
        assert_relative_eq!(
            sol.design.rate.newton_meters_per_radian(),
            0.5085,
            max_relative = 1e-9
        );
        assert_eq!(sol.design.load_points.len(), 1);
    }

    #[test]
    fn mass_is_policy_independent_and_compact_d_is_smaller() {
        // THE D-independence property: same request, both policies → same winning
        // wire, same mass (the analytic identity), Compact's D ≤ MaxMargin's D.
        let m = music_wire();
        let max_margin = solve_min_weight(&m, &base_request()).expect("feasible");
        let compact = solve_min_weight(
            &m,
            &TorMinWeightRequest {
                dia_policy: DiaPolicy::Compact,
                ..base_request()
            },
        )
        .expect("feasible");
        assert_eq!(
            max_margin.design.inputs.wire_dia,
            compact.design.inputs.wire_dia
        );
        assert_relative_eq!(max_margin.mass_kg, compact.mass_kg, max_relative = 1e-9);
        assert!(
            compact.design.inputs.mean_dia.meters()
                <= max_margin.design.inputs.mean_dia.meters()
        );
        // Low moment → stress fine at the c_min floor → Compact binds on Index too.
        assert_eq!(compact.binding, TorBindingConstraint::Index);
        assert_relative_eq!(
            compact.design.inputs.mean_dia.meters(),
            4.0 * 0.0015,
            max_relative = 1e-12
        );
    }

    #[test]
    fn compact_stress_governed_lands_on_the_allowable() {
        // Pick M so t = allow·π·d³/(32·M) falls strictly between K_bi(c_max) and
        // K_bi(c_min) for the single candidate: stress governs D. The observable
        // oracle: binding = BendingStress and the design's inner-fiber stress at
        // max moment sits ON the allowable (pct_bending_allow ≈ 1) — a property
        // check, not a re-derivation of the quadratic.
        let m = music_wire();
        let d = Length::from_millimeters(2.0);
        let mts = m.min_tensile_strength(d).unwrap().pascals();
        let allow = m.allowable_pct_bending * mts;
        let t_target = 1.15; // between K_bi(12) ≈ 1.066 and K_bi(4) ≈ 1.229
        let moment_nm = allow * PI * 0.002_f64.powi(3) / (32.0 * t_target);
        let req = TorMinWeightRequest {
            dia_policy: DiaPolicy::Compact,
            max_moment: Moment::from_newton_meters(moment_nm),
            candidate_diameters: vec![d],
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("stress-governed but feasible");
        assert_eq!(sol.binding, TorBindingConstraint::BendingStress);
        let lp = &sol.design.load_points[0];
        assert_relative_eq!(lp.pct_bending_allow, 1.0, max_relative = 1e-6);
        let c = sol.design.index;
        assert!(c > 4.0 && c < 12.0, "stress-governed C strictly inside bounds, got {c}");
    }

    #[test]
    fn max_margin_od_cap_binds_outer_diameter() {
        // Cap below c_max·d + d: MaxMargin's ceiling comes from the cap.
        // d = 1.5 mm, cap 12 mm → D = 10.5 mm (C = 7, inside bounds).
        let m = music_wire();
        let req = TorMinWeightRequest {
            max_outer_dia: Some(Length::from_millimeters(12.0)),
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("feasible");
        assert_eq!(sol.binding, TorBindingConstraint::OuterDiameter);
        assert_relative_eq!(
            sol.design.inputs.mean_dia.meters() + sol.design.inputs.wire_dia.meters(),
            0.012,
            max_relative = 1e-12
        );
    }

    #[test]
    fn od_cap_below_index_floor_skips_candidate() {
        // Cap so tight that (OD − d)/d < c_min for the small candidates: they skip;
        // with NO candidate clearing it, the request is Infeasible.
        let m = music_wire();
        let req = TorMinWeightRequest {
            // For d = 1.5: OD−d = 4.5 → C = 3 < 4. For 2.0: 4.0 → C = 2. For 2.5:
            // 3.5 → C = 1.4. All below c_min = 4 → Infeasible.
            max_outer_dia: Some(Length::from_millimeters(6.0)),
            ..base_request()
        };
        match solve_min_weight(&m, &req) {
            Err(crate::SpringError::Infeasible(msg)) => {
                assert!(msg.contains("no candidate wire diameter"), "{msg}")
            }
            other => panic!("expected Infeasible, got {other:?}"),
        }
    }

    #[test]
    fn leg_mass_follows_the_two_thirds_relationship() {
        // Legs totalling L add exactly ρ·(π/4)d²·(⅔·L) of mass: L of straight wire
        // MINUS L/3 of body shortening (the leg term's coil equivalent).
        let m = music_wire();
        let no_legs = solve_min_weight(&m, &base_request()).expect("feasible");
        let legs = solve_min_weight(
            &m,
            &TorMinWeightRequest {
                leg1: Length::from_millimeters(15.0),
                leg2: Length::from_millimeters(15.0),
                ..base_request()
            },
        )
        .expect("feasible");
        assert_eq!(no_legs.design.inputs.wire_dia, legs.design.inputs.wire_dia);
        let d = no_legs.design.inputs.wire_dia.meters();
        let delta = m.density.kg_per_m3() * (PI / 4.0) * d * d * (2.0 / 3.0) * 0.030;
        assert_relative_eq!(legs.mass_kg - no_legs.mass_kg, delta, max_relative = 1e-9);
    }

    #[test]
    fn legs_that_consume_all_coils_skip_to_the_next_candidate() {
        // N_b > 0 ⟺ E·d⁴/(denom·k′) > (L₁+L₂)/(3π), D-independent. With legs
        // totalling 1.2 m at k′ = 0.5085: d = 2.0 mm fails (0.100 < 0.127) while
        // d = 2.5 mm passes (0.244 > 0.127) — the larger candidate must win.
        let m = music_wire();
        let req = TorMinWeightRequest {
            leg1: Length::from_millimeters(600.0),
            leg2: Length::from_millimeters(600.0),
            candidate_diameters: vec![
                Length::from_millimeters(2.0),
                Length::from_millimeters(2.5),
            ],
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("the larger candidate is feasible");
        assert_relative_eq!(
            sol.design.inputs.wire_dia.meters(),
            0.0025,
            max_relative = 1e-12
        );
        assert!(sol.design.inputs.body_coils > 0.0);
    }

    #[test]
    fn friction_model_changes_mass_by_the_denominator_ratio() {
        // No legs: mass ∝ 1/denom, so mass_pure/mass_shigley = (2π·10.8)/64.
        let m = music_wire();
        let pure = solve_min_weight(&m, &base_request()).expect("feasible");
        let shigley = solve_min_weight(
            &m,
            &TorMinWeightRequest {
                friction_model: FrictionModel::ShigleyFriction,
                ..base_request()
            },
        )
        .expect("feasible");
        assert_relative_eq!(
            pure.mass_kg / shigley.mass_kg,
            TAU * 10.8 / 64.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn overstressed_everywhere_is_infeasible_and_out_of_range_skips() {
        let m = music_wire();
        // A moment far beyond any candidate's allowable at every allowed D.
        let req = TorMinWeightRequest {
            max_moment: Moment::from_newton_meters(1.0e6),
            ..base_request()
        };
        assert!(matches!(
            solve_min_weight(&m, &req),
            Err(crate::SpringError::Infeasible(_))
        ));
        // A candidate outside the material's diameter range skips (not fatal) while
        // a valid candidate wins.
        let req = TorMinWeightRequest {
            candidate_diameters: vec![
                Length::from_millimeters(50.0), // far outside music-wire range
                Length::from_millimeters(2.0),
            ],
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("valid candidate wins");
        assert_relative_eq!(
            sol.design.inputs.wire_dia.meters(),
            0.002,
            max_relative = 1e-12
        );
    }

    #[test]
    fn arbor_passthrough_surfaces_engine_advisories() {
        // An arbor slightly LARGER than the wound-up inner diameter at max moment
        // trips the engine's wind-down advisory (design.rs fires it when
        // wound_inner ≤ arbor; message contains "arbor"). Advisory, not a
        // constraint — the solve still succeeds. Assert the arbor-specific message,
        // NOT merely non-empty status: index_caution could populate messages on its
        // own and make a bare non-empty check vacuous.
        let m = music_wire();
        let base = solve_min_weight(&m, &base_request()).expect("feasible");
        let wound_id = base.design.load_points[0].wound_inner_dia.meters();
        let req = TorMinWeightRequest {
            arbor_dia: Some(Length::from_meters(wound_id * 1.001)),
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("arbor is advisory, not a constraint");
        assert!(
            sol.design
                .status
                .messages
                .iter()
                .any(|msg| msg.message.contains("arbor")),
            "arbor above the wound ID must surface the engine's wind-down advisory; got: {:?}",
            sol.design.status.messages
        );
    }
```

Update `well_formed_request_reaches_the_search` (Task 1's placeholder expectation): DELETE it — `golden_oracle_smallest_feasible_candidate_wins_max_margin` supersedes it (same request now solves).

- [ ] **Step 2: Run to verify they fail** — `cargo test -p springcore --lib torsion::optimize` → new tests FAIL (`Infeasible` from the skeleton; helpers missing).

- [ ] **Step 3: Implement the search**

Add the two helpers (above `solve_min_weight`); extend the mechanics import:

```rust
use crate::torsion::mechanics::{
    active_coils_for_rate, active_coils_with_legs, bending_stress_inner, kbi_factor,
    FrictionModel,
};
```

```rust
/// The index C at which K_bi(C) == t, for t > 1 — the Compact policy's
/// stress-governed bound. K_bi(C) = t reduces to the quadratic
/// C²(4−4t) + C(4t−1) − 1 = 0 (Shigley Eq. 10-43 rearranged); for t > 1 exactly
/// one root lies in C > 1 (K_bi is a decreasing bijection (1,∞) → (1,∞)).
/// t ≤ 1 CANNOT reach this function: K_bi > 1 for every finite C, so the
/// ceiling-stress feasibility check already skipped such candidates — callers
/// guarantee t > 1, no special-case branch exists (spec: documented, not branched).
fn compact_index_for_stress(t: f64) -> f64 {
    let a = 4.0 - 4.0 * t; // < 0 for t > 1
    let b = 4.0 * t - 1.0;
    let disc = b * b + 4.0 * a; // b² − 4·a·(−1)
    // Of the two real roots, the one in C > 1 is (−b − √disc)/(2a) with a < 0
    // (the "−" branch divided by a negative leading coefficient).
    (-b - disc.sqrt()) / (2.0 * a)
}

/// Wire mass of a torsion design: ρ · (π/4)·d² · (π·D·N_b + L₁ + L₂) — the body
/// helix plus the two straight legs, from the ACTUAL chosen geometry (identical to
/// the spec's closed form by the D-independence identity; single-sourced with the
/// engine's geometry rather than re-deriving the analytic expression).
fn wire_mass(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    body_coils: f64,
    leg1: Length,
    leg2: Length,
) -> f64 {
    let d = wire_dia.meters();
    let l_wire =
        std::f64::consts::PI * mean_dia.meters() * body_coils + leg1.meters() + leg2.meters();
    material.density.kg_per_m3() * (std::f64::consts::PI / 4.0) * d * d * l_wire
}
```

Replace the skeleton's `let best: Option<TorMinWeightSolution> = None;` + comment with the search:

```rust
    let (c_min, c_max) = req.index_bounds;
    let mut best: Option<TorMinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        // Material range gate: out-of-range diameters skip like the siblings.
        let Ok(mts) = material.min_tensile_strength(d) else {
            continue;
        };
        let allow = material.allowable_pct_bending * mts.pascals();
        let dm = d.meters();

        // Allowed mean-diameter interval; an OD cap below the index floor skips.
        let dm_lo = c_min * dm;
        let mut dm_hi = c_max * dm;
        let mut hi_is_od_capped = false;
        if let Some(od) = req.max_outer_dia {
            let capped = od.meters() - dm;
            if capped < dm_hi {
                dm_hi = capped;
                hi_is_od_capped = true;
            }
        }
        if dm_hi < dm_lo {
            continue;
        }

        // ONE stress evaluation decides feasibility: K_bi is monotone decreasing in
        // C (module doc), so σᵢ over [dm_lo, dm_hi] is minimal at dm_hi. If even
        // the ceiling exceeds the allowable, no allowed D works.
        let stress_at_hi =
            bending_stress_inner(req.max_moment, Length::from_meters(dm_hi), d).pascals();
        if stress_at_hi > allow {
            continue;
        }

        // Choose D per policy (mass is D-independent — module doc).
        let (mean_m, binding) = match req.dia_policy {
            DiaPolicy::MaxMargin => (
                dm_hi,
                if hi_is_od_capped {
                    TorBindingConstraint::OuterDiameter
                } else {
                    TorBindingConstraint::Index
                },
            ),
            DiaPolicy::Compact => {
                // t = allow·π·d³/(32·M): the K_bi value at which σᵢ == allowable.
                let t = allow * std::f64::consts::PI * dm.powi(3)
                    / (32.0 * req.max_moment.newton_meters());
                if kbi_factor(c_min) <= t {
                    // Stress already satisfied at the index floor (K_bi decreasing):
                    // the floor is the most compact allowed coil.
                    (dm_lo, TorBindingConstraint::Index)
                } else {
                    // Stress governs: the ceiling check above guarantees t > K_bi at
                    // dm_hi ≥ … > 1, so compact_index_for_stress's t > 1 contract
                    // holds and C_stress lies in (c_min, dm_hi/dm].
                    (
                        compact_index_for_stress(t) * dm,
                        TorBindingConstraint::BendingStress,
                    )
                }
            }
        };
        let mean = Length::from_meters(mean_m);

        // Body coils from the rate inversion minus the leg term. N_b ≤ 0 (or
        // non-finite) is D-independent (module doc) — the candidate cannot meet the
        // rate with these legs at any D; skip.
        let na = active_coils_for_rate(
            material.youngs_modulus,
            d,
            mean,
            req.required_rate,
            req.friction_model,
        );
        let body_coils = na - active_coils_with_legs(0.0, req.leg1, req.leg2, mean);
        if !(body_coils.is_finite() && body_coils > 0.0) {
            continue;
        }

        // Full engine backstop; arbor advisories ride along in the design status.
        let Ok(design) = solve_forward(
            material,
            TorsionInputs {
                wire_dia: d,
                mean_dia: mean,
                body_coils,
                leg1: req.leg1,
                leg2: req.leg2,
                arbor_dia: req.arbor_dia,
            },
            &[req.max_moment],
            req.friction_model,
        ) else {
            continue;
        };

        let mass_kg = wire_mass(material, d, mean, body_coils, req.leg1, req.leg2);
        if best.as_ref().is_none_or(|b| mass_kg < b.mass_kg) {
            best = Some(TorMinWeightSolution {
                design,
                binding,
                mass_kg,
            });
        }
    }
```

(The trailing `best.ok_or_else(...)` from Task 1 stays as-is. `use crate::units::Stress;` is NOT needed — `pascals()` comes off the returned `Stress` value directly.)

- [ ] **Step 4: Run to verify pass** — `cargo test -p springcore --lib torsion::optimize` → PASS (15 tests); `cargo test -p springcore --lib` all green.

- [ ] **Step 5: Mutation-check + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Kill map: interval mutants (min/</swap) → od_cap tests; stress `>` boundary →
# compact_stress_governed (pct ≈ 1.0 pins the equality direction); policy-swap →
# mass_is_policy_independent (D differs); quadratic-coefficient mutants →
# compact_stress_governed's pct property; N_b guard → legs_that_consume_all_coils;
# wire_mass formula → golden oracle + leg two-thirds + friction ratio; keep-lightest
# `<` → golden oracle (smallest d must win over later feasible candidates).
# If any survives, strengthen the mapped test; never loosen.
git add springcore/src/torsion/optimize.rs
git commit -m "feat(torsion): analytic minimum-weight search — policy D, closed-form Compact"
```

---

### Task 3: Full local gate (+ final panel by the controller)

**Files:** none new — verification only.

- [ ] **Step 1: Full gate**

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
typos
cargo test --workspace --all-features
git diff origin/main -- > /tmp/pr.diff
cargo mutants --in-diff /tmp/pr.diff --no-shuffle -j 2 --package springcore --all-features
# Expected: everything green; literal 0 survivors across the branch diff.
```

- [ ] **Step 2: Final whole-branch review** — the controller dispatches the adversarial panel (general-code, architect, simplifier, MANDATORY input-domain adversary — with explicit numerical-correctness attention on `compact_index_for_stress` and the monotonicity assumptions; no persistence reviewer, no persistence surface), cycles to convergence, then pushes and opens the PR.

---

## Notes for the implementer

- **The analytic identities are the spec, not an optimization to "improve":** do not
  add root-finding, do not re-evaluate stress across the interval, do not branch on
  t ≤ 1 (unreachable past the ceiling check — the doc comment carries the proof
  obligation).
- **`wire_mass` uses the CHOSEN geometry**, not the closed form — the tests pin their
  equality (golden oracle vs the analytic expression); duplicating the closed form in
  the implementation would silently decouple mass from the shipped geometry.
- **Skip vs fail:** per-candidate problems (out-of-range MTS, capped-below-floor,
  overstressed, N_b ≤ 0, forward-solve error) SKIP the candidate; only request-shape
  problems fail fast as `InconsistentInputs` (Task 1's block), and an empty feasible
  set is `Infeasible`.
- `is_none_or` is stable since Rust 1.82 (< MSRV 1.88) — fine to use.
