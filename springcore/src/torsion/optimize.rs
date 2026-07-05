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

use std::f64::consts::PI;

use crate::material::Material;
use crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs};
use crate::torsion::mechanics::{
    active_coils_for_rate, active_coils_with_legs, bending_stress_inner, kbi_factor, FrictionModel,
};
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
    /// Never reports [`TorBindingConstraint::OuterDiameter`]: an OD cap only
    /// narrows the feasible ceiling, while Compact's D lands on the stress bound
    /// or the index floor.
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
    /// The outer-diameter cap set D (MaxMargin with OD − d < c_max·d; never
    /// reported under Compact — see [`DiaPolicy::Compact`]).
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
    /// The fully solved torsion design at the winning wire diameter.
    pub design: TorsionDesign,
    /// Which constraint bound the chosen mean diameter (see [`TorBindingConstraint`]).
    pub binding: TorBindingConstraint,
    /// Total wire mass in kilograms (body helix plus both straight legs).
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

/// The index C at which K_bi(C) == t, for t > 1 — the Compact policy's
/// stress-governed bound. K_bi(C) = t reduces to the quadratic
/// C²(4−4t) + C(4t−1) − 1 = 0 (Shigley Eq. 10-43 rearranged); for t > 1 exactly
/// one root lies in C > 1 (K_bi is a decreasing bijection (1,∞) → (1,∞)).
/// t ≤ 1 CANNOT reach this function: K_bi > 1 for every finite C, so the
/// ceiling-stress feasibility check already skipped such candidates — callers
/// guarantee t > 1, no special-case branch exists (spec: documented, not branched).
/// (One triply-pathological corner — a synthetic material plus c_max large enough
/// to overflow dm_hi to +Inf — can bypass the NaN-comparing ceiling check and reach
/// here with t ≤ 1; the resulting non-positive or non-finite mean is rejected by
/// solve_forward's geometry guard, so the contract is about the reachable domain,
/// not a safety boundary.)
fn compact_index_for_stress(t: f64) -> f64 {
    let a = 4.0 - 4.0 * t; // < 0 for t > 1
    let b = 4.0 * t - 1.0;
    let disc = b * b + 4.0 * a; // b² − 4ac with constant term c = −1

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
    let l_wire = PI * mean_dia.meters() * body_coils + leg1.meters() + leg2.meters();
    material.density.kg_per_m3() * (PI / 4.0) * d * d * l_wire
}

/// Pick the lightest feasible torsion design over the candidate wire diameters.
/// See the module doc for the analytic structure; the design spec records the
/// derivation.
pub fn solve_min_weight(
    material: &Material,
    req: &TorMinWeightRequest,
) -> Result<TorMinWeightSolution> {
    validate_request(req)?;
    let (c_min, c_max) = req.index_bounds;
    let mut best: Option<TorMinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        // Material range gate: out-of-range diameters skip like the siblings.
        let Ok(mts) = material.min_tensile_strength(d) else {
            continue;
        };
        let allow = material.allowable_pct_bending * mts.pascals();
        // `dw` is the WIRE diameter in meters; dm_lo/dm_hi below are the derived
        // MEAN-diameter bounds (C·d), matching the dm = mean-dia convention.
        let dw = d.meters();

        // Allowed mean-diameter interval; an OD cap below the index floor skips.
        let dm_lo = c_min * dw;
        let mut dm_hi = c_max * dw;
        let mut hi_is_od_capped = false;
        if let Some(od) = req.max_outer_dia {
            let capped = od.meters() - dw;
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
                let t = allow * PI * dw.powi(3) / (32.0 * req.max_moment.newton_meters());
                if kbi_factor(c_min) <= t {
                    // Stress already satisfied at the index floor (K_bi decreasing):
                    // the floor is the most compact allowed coil.
                    (dm_lo, TorBindingConstraint::Index)
                } else {
                    // Stress governs: the ceiling check above guarantees t > K_bi at
                    // dm_hi ≥ … > 1, so compact_index_for_stress's t > 1 contract
                    // holds and C_stress lies in (c_min, dm_hi/dw].
                    (
                        compact_index_for_stress(t) * dw,
                        TorBindingConstraint::BendingStress,
                    )
                }
            }
        };
        let mean = Length::from_meters(mean_m);

        // N_b ≤ 0 or non-finite (legs consume every coil the rate allows — a
        // D-independent condition) is rejected by solve_forward's body-coils
        // guard below; the Err → continue skip is the single path, deliberately
        // NOT pre-guarded here (a redundant pre-check is mutation-equivalent
        // to the backstop and ungateable).
        let na = active_coils_for_rate(
            material.youngs_modulus,
            d,
            mean,
            req.required_rate,
            req.friction_model,
        );
        // body_coils = 0 extracts JUST the leg coil-equivalent (L₁+L₂)/(3πD)
        // (Shigley Eq. 10-50 is linear in N_b), single-sourcing the leg formula.
        let leg_term = active_coils_with_legs(0.0, req.leg1, req.leg2, mean);
        let body_coils = na - leg_term;

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

    best.ok_or_else(|| {
        SpringError::Infeasible(format!(
            "no candidate wire diameter (of {}) yields a feasible torsion design",
            req.candidate_diameters.len()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::music_wire;
    use crate::torsion::FrictionModel;
    use crate::units::{AngularRate, Length, Moment};
    use approx::assert_relative_eq;
    use std::f64::consts::{PI, TAU};

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
            (1.0, 12.0), // c_min == 1: K_bi undefined at 1, monotone only above
            (0.5, 12.0), // below 1
            (6.0, 6.0),  // not strictly increasing
            (8.0, 4.0),  // inverted
            (f64::NAN, 12.0),
            (4.0, f64::INFINITY),
        ] {
            let req = TorMinWeightRequest {
                index_bounds: (lo, hi),
                ..base_request()
            };
            match solve_min_weight(&m, &req) {
                Err(crate::SpringError::InconsistentInputs(msg)) => {
                    assert!(msg.contains("index bounds"), "bounds=({lo},{hi}): {msg}")
                }
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
    fn golden_oracle_smallest_feasible_candidate_wins_max_margin() {
        // Low moment → every candidate is stress-feasible → the smallest d wins
        // (mass strictly increasing in d). MaxMargin, no cap → D = c_max·d,
        // binding = Index. Mass checked against the closed form
        // ρ·(π/4)d²·(π·E·d⁴/(64·k′)) (no legs), reading ρ and E from the material
        // so the oracle is exact without hardcoding material constants.
        let m = music_wire();
        let sol = solve_min_weight(&m, &base_request()).expect("feasible");
        let d = 0.0015_f64;
        assert_relative_eq!(sol.design.inputs.wire_dia.meters(), d, max_relative = 1e-12);
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
            compact.design.inputs.mean_dia.meters() <= max_margin.design.inputs.mean_dia.meters()
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
        assert!(
            c > 4.0 && c < 12.0,
            "stress-governed C strictly inside bounds, got {c}"
        );
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
            candidate_diameters: vec![Length::from_millimeters(2.0), Length::from_millimeters(2.5)],
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
    fn lightest_wins_regardless_of_candidate_order() {
        // Candidates in DESCENDING order: the keep-lightest comparison must update
        // best even when a later candidate is lighter than the stored one. With `==`
        // instead of `<` the first (heaviest) candidate would stick — this kills that
        // mutant directly, since mass equality across different wire diameters is
        // impossible (mass ∝ d² × 1/d⁴ = 1/d² for the body alone).
        let m = music_wire();
        let req = TorMinWeightRequest {
            candidate_diameters: vec![
                Length::from_millimeters(2.5), // heaviest first
                Length::from_millimeters(2.0),
                Length::from_millimeters(1.5), // lightest last
            ],
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("feasible");
        assert_relative_eq!(
            sol.design.inputs.wire_dia.meters(),
            0.0015,
            max_relative = 1e-12
        );
    }

    #[test]
    fn od_cap_exactly_at_index_ceiling_binding_is_index_not_od() {
        // d = 1/512 m = 2^{-9} m (exact). With c_max = 12: dm_hi = 12/512 (exact).
        // OD = 13/512 m → capped = 13/512 − 1/512 = 12/512 = dm_hi (bit-exact).
        // Guard `capped < dm_hi`: 12/512 < 12/512 → false → hi_is_od_capped stays false
        // → MaxMargin reports binding = Index.
        // The `<→<=` mutant flips the guard true → hi_is_od_capped = true →
        // binding = OuterDiameter — this test kills the `capped < dm_hi` mutant.
        let m = music_wire();
        let d_val = 1.0_f64 / 512.0; // 2^{-9}: exactly representable
        let c_max = 12.0_f64;
        let od = Length::from_meters((c_max + 1.0) * d_val); // 13/512 m — exact
        let req = TorMinWeightRequest {
            candidate_diameters: vec![Length::from_meters(d_val)],
            max_outer_dia: Some(od),
            dia_policy: DiaPolicy::MaxMargin,
            ..base_request()
        };
        let sol = solve_min_weight(&m, &req).expect("feasible: stress well below allowable");
        assert_eq!(
            sol.binding,
            TorBindingConstraint::Index,
            "capped == dm_hi: the OD cap at the index ceiling must NOT override it; \
             binding must be Index, not OuterDiameter"
        );
    }

    #[test]
    fn od_cap_exactly_at_index_floor_still_feasible() {
        // d = 1/512 m = 2^{-9} m (exact). c_min = 4, c_max = 12.
        // dm_lo = 4/512 (exact). Initial dm_hi = 12/512 (exact).
        // OD = 5/512 m → capped = 4/512 = dm_lo, so dm_hi is capped to dm_lo.
        // Guard `dm_hi < dm_lo`: 4/512 < 4/512 → false → proceeds with C = c_min.
        // The `<→<=` mutant makes the guard true → skips → Infeasible — this test
        // kills the `dm_hi < dm_lo` mutant.
        let m = music_wire();
        let d_val = 1.0_f64 / 512.0; // 2^{-9}: exactly representable
        let c_min = 4.0_f64;
        let c_max = 12.0_f64;
        // OD = (c_min + 1) * d = 5/512 m → capped = c_min * d = dm_lo = dm_hi after cap.
        let od = Length::from_meters((c_min + 1.0) * d_val);
        let req = TorMinWeightRequest {
            candidate_diameters: vec![Length::from_meters(d_val)],
            max_outer_dia: Some(od),
            index_bounds: (c_min, c_max),
            ..base_request()
        };
        // Original: dm_hi == dm_lo, guard false → proceeds → feasible.
        // Mutant:   guard true → skips → Infeasible.
        solve_min_weight(&m, &req)
            .expect("dm_hi == dm_lo is NOT a skip condition; the boundary design is feasible");
    }

    #[test]
    fn stress_exactly_at_allowable_still_proceeds() {
        // d = 1/512 m = 2^{-9} m (exact). c_max = 2 → kbi(2) = 13/8 = 1.625 (exact).
        // M = allow·π·d³/(32·kbi): at d³ = 2^{-27} the π/π and kbi/kbi factors cancel
        // bit-exactly, so bending_stress_inner(...).pascals() == allow in IEEE 754.
        // This test is the proof; it is DETERMINISTIC but EMPIRICALLY TUNED to the
        // current music-wire MTS/allowable constants (unlike the OD-cap dyadic tests,
        // which are exact by construction). If those material constants ever drift by
        // a ULP, this test may silently stop killing the `> → >=` ceiling mutant —
        // re-tune the moment to restore bit-exactness at the boundary.
        // Guard `stress_at_hi > allow`: allow > allow → false → proceeds → Ok.
        // The `>→>=` mutant: allow >= allow → true → skips → Infeasible — this test
        // kills the `stress_at_hi > allow` mutant.
        let m = music_wire();
        let d_val = 1.0_f64 / 512.0; // 2^{-9}: exactly representable
        let d = Length::from_meters(d_val);
        let mts_pa = m.min_tensile_strength(d).unwrap().pascals();
        let allow = m.allowable_pct_bending * mts_pa;
        // kbi(2.0) = (4·4 − 2 − 1)/(4·2·1) = 13/8 = 1.625 — exact in f64.
        let c_max_val = 2.0_f64;
        let kbi_val =
            (4.0 * c_max_val * c_max_val - c_max_val - 1.0) / (4.0 * c_max_val * (c_max_val - 1.0));
        let moment_nm = allow * PI * d_val.powi(3) / (32.0 * kbi_val);
        let req = TorMinWeightRequest {
            candidate_diameters: vec![d],
            index_bounds: (1.5, c_max_val),
            max_moment: Moment::from_newton_meters(moment_nm),
            max_outer_dia: None,
            ..base_request()
        };
        // Original: stress_at_hi == allow → `>` is false → proceeds → Ok.
        // Mutant:   `>=` is true → skips → Infeasible.
        solve_min_weight(&m, &req)
            .expect("stress == allow passes the strict `>` guard; the `>=` mutant rejects it");
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
}
