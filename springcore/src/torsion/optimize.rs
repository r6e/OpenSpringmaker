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
use crate::torsion::design::TorsionDesign;
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
    _material: &Material,
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
