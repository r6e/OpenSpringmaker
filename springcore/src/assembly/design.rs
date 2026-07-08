//! Assembly solve: two-pass composition over the compression engine plus
//! derived combination outputs. Sources at each site; module docs cover the
//! model boundary and omissions.

use crate::design::{index_caution_labeled, DesignStatus, Severity, SpringDesign, StatusMessage};
use crate::end_type::EndType;
use crate::material_store::MaterialStore;
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
    /// Assembly-level state at each applied load: force, deflection (F/k_assembly), assembly length (free_length − deflection).
    pub load_points: Vec<AssemblyLoadPoint>,
}

/// Wrap a member-level error with its 1-based attribution, preserving the
/// inner error's structure (a UI layer re-localizes `DiameterOutOfRange`).
/// The `Member` variant's `Display` reproduces the previous flattened string
/// byte-for-byte, so error *messages* are unchanged; only the *structure* is
/// richer.
fn member_error(index: usize, err: SpringError) -> SpringError {
    SpringError::Member {
        index,
        source: Box::new(err),
    }
}

pub fn solve_assembly(
    materials: &MaterialStore,
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
        if inputs.members.iter().any(|m| m.free_length.meters() != l0) {
            return Err(SpringError::InconsistentInputs(
                "nested members must share a free length (staged engagement is not modeled)".into(),
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
    // Nested rate sum bound once; reused for k_total and per-member shares so
    // both see the same f64 accumulation (bit-identical, same operands/order).
    let nested_rate_sum: f64 = rates.iter().sum();
    let k_total = match inputs.topology {
        // k = Σkᵢ (Ch. 4, generalized to N by the same equilibrium argument).
        Topology::Nested => nested_rate_sum,
        // 1/k = Σ 1/kᵢ (Eq. 8-15, generalized to N).
        Topology::Series => 1.0 / rates.iter().map(|k| 1.0 / k).sum::<f64>(),
    };

    // Pass 2 — solve each member at its share for the real per-member state.
    let mut members = Vec::with_capacity(inputs.members.len());
    for (i, m) in inputs.members.iter().enumerate() {
        let share = match inputs.topology {
            Topology::Nested => rates[i] / nested_rate_sum,
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
    let travel_limit_deflection = Length::from_meters(travel_limit_force.newtons() / k_total);

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
pub fn evaluate_status(design: &AssemblyDesign, materials: &MaterialStore) -> DesignStatus {
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

    fn materials() -> crate::MaterialStore {
        crate::MaterialStore::new(crate::MaterialSet::load_default())
    }

    fn solve(
        topology: Topology,
        members: Vec<AssemblyMember>,
        loads: &[f64],
    ) -> crate::Result<AssemblyDesign> {
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
        assert_relative_eq!(
            asm.load_points[0].deflection.millimeters(),
            10.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            asm.members[0].design.load_points[0]
                .deflection
                .millimeters(),
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
        assert_relative_eq!(
            asm.load_points[0].deflection.millimeters(),
            20.0,
            max_relative = 1e-9
        );
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
        assert_relative_eq!(
            asm.members[0].share_fraction,
            k0 / (k0 + k1),
            max_relative = 1e-12
        );
        assert_relative_eq!(
            asm.members[1].share_fraction,
            k1 / (k0 + k1),
            max_relative = 1e-12
        );
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
        // Nested baseline pair: travel ≈ 36 mm at k ≈ 4000 N/m. Derive the
        // exact tlf from the geometry (IEEE 754 prevents pinning 144.0 N
        // exactly — k is not representable as a power-of-two multiple).
        let pair = || vec![baseline_member(), baseline_member()];
        let ref_asm = solve(Topology::Nested, pair(), &[1.0]).unwrap();
        let tlf = ref_asm.travel_limit_force.newtons();
        // Exactly at the travel limit: load == tlf → deflection == tld → no warn.
        let at = solve(Topology::Nested, pair(), &[tlf]).unwrap();
        let status = evaluate_status(&at, &materials());
        assert!(
            !has_message(&status, "exceeds the travel limit"),
            "exactly at the limit must not warn (strict >)"
        );
        // Over the limit: load slightly above tlf warns.
        let over_f = tlf + 0.1;
        let over = solve(Topology::Nested, pair(), &[over_f]).unwrap();
        let status = evaluate_status(&over, &materials());
        assert!(has_message(
            &status,
            "load point 1 exceeds the travel limit"
        ));
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
        assert!(!has_message(
            &evaluate_status(&asm, &materials()),
            "nested interference"
        ));
        // Series: identical geometry is legal, no clearance semantics.
        let asm = solve(Topology::Series, vec![baseline_member(), inner_at], &[10.0]).unwrap();
        assert!(!has_message(
            &evaluate_status(&asm, &materials()),
            "nested interference"
        ));
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
        // Overstress member 2 only: thin wire + large mean diameter drives
        // stress above the torsion allowable (45% MTS for Music Wire) at
        // 40 N. d=0.5mm, D=20mm → C=40, K≈1.03; stress ≈ 2100 MPa which
        // is 86% of MTS (≫ 45%). Member 1 (baseline) is well under limit.
        let small = AssemblyMember {
            wire_dia: Length::from_millimeters(0.5),
            mean_dia: Length::from_millimeters(20.0),
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
            Err(crate::SpringError::Member { index, source }) => {
                let inner = match *source {
                    crate::SpringError::InconsistentInputs(m) => m,
                    other => other.to_string(),
                };
                format!("member {}: {inner}", index + 1)
            }
            other => panic!("expected InconsistentInputs or Member, got {other:?}"),
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
            msg(solve(
                Topology::Nested,
                vec![baseline_member(), long],
                &[10.0]
            )),
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
        let m = msg(solve(
            Topology::Series,
            vec![baseline_member(), bad],
            &[10.0],
        ));
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
            m.starts_with("member 1: wire diameter") && m.contains("outside valid range"),
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
        let asm = solve(
            Topology::Nested,
            vec![baseline_member(), stainless],
            &[10.0],
        )
        .unwrap();
        assert_eq!(asm.members[0].material_name, "Music Wire");
        assert_eq!(asm.members[1].material_name, "Stainless 302");
    }

    #[test]
    fn huge_load_trips_the_output_guard() {
        // 1e305 N overflows shear stress before the assembly-level guard fires
        // (design.rs catches non-finite load-point stress and wraps it through
        // member_error). The assembly guard is still present and catches any
        // case that slips past member-level checks; this fixture pins the
        // actual end-to-end error path.
        let m = msg(solve(Topology::Nested, vec![baseline_member()], &[1e305]));
        assert_eq!(
            m,
            "member 1: solve produced a non-finite result (inputs exceed the representable range)"
        );
    }

    // ── Mutation-killing supplement ─────────────────────────────────────────

    #[test]
    fn zero_load_is_accepted() {
        // Load of 0.0 N is finite and non-negative → must be accepted.
        // Kills the `< 0.0 → <= 0.0` mutant in the load guard (line 113).
        let asm = solve(Topology::Nested, vec![baseline_member()], &[0.0]).unwrap();
        assert_relative_eq!(
            asm.load_points[0].deflection.millimeters(),
            0.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn series_tie_limiting_member_resolves_to_lowest_index() {
        // Two members with identical bottoming forces: k1*(L01-Ls1) == k2*(L02-Ls2).
        // Two baseline members in series: both have k≈2000, travel=36mm → F_bottom≈72N.
        // The lowest index (0) must win, not the last seen.
        // Kills the `< → <=` mutant in the series travel-limit fold (line 235).
        let asm = solve(
            Topology::Series,
            vec![baseline_member(), baseline_member()],
            &[1.0],
        )
        .unwrap();
        assert_eq!(asm.limiting_member, 0, "tie breaks to lowest index");
    }

    #[test]
    fn load_point_length_is_free_minus_deflection() {
        // load_point.length = free_length − deflection.
        // Kills the `- → +` and `- → /` mutants in load_point construction (line 253).
        let asm = solve(Topology::Nested, vec![baseline_member()], &[10.0]).unwrap();
        let lp = &asm.load_points[0];
        assert_relative_eq!(
            lp.length.meters(),
            asm.free_length.meters() - lp.deflection.meters(),
            max_relative = 1e-12
        );
        // Also verify the magnitude: 60mm − 5mm = 55mm.
        assert_relative_eq!(lp.length.millimeters(), 55.0, max_relative = 1e-9);
    }

    #[test]
    fn torsion_stress_exactly_at_allowable_produces_no_warning() {
        // Load chosen so that pct_mts == allowable_pct_torsion (0.45) exactly
        // in IEEE 754 double precision (for Music Wire, d=2mm, D=20mm baseline).
        // Formula: F = pct * MTS * π * d³ / (K * 8 * D).
        // Derived from springcore's corrected_shear_stress: K * 8 * F * D / (π * d³).
        // The round-trip is exact for this geometry (verified empirically).
        // Exactly at limit must NOT warn (strict > semantics).
        // Kills the `> → >=` mutant in the load-point torsion check (line 340).
        use std::f64::consts::PI;
        let d_mm = 2.0_f64;
        let d = d_mm / 1000.0;
        let dm = 20.0_f64 / 1000.0;
        let c = dm / d; // index = 10
        let k_berg = (4.0 * c + 2.0) / (4.0 * c - 3.0);
        let mts_pa = 2211.0_f64 / d_mm.powf(0.145) * 1e6; // Music Wire PowerLaw in Pa
        let allowable_pct = 0.45_f64;
        // F such that K*8*F*D/(π*d³) == allowable_pct * mts
        let f_at = allowable_pct * mts_pa * PI * d.powi(3) / (k_berg * 8.0 * dm);
        let asm = solve(Topology::Nested, vec![baseline_member()], &[f_at]).unwrap();
        let status = evaluate_status(&asm, &materials());
        // The travel limit warning fires (f_at > travel limit force) — ignore it;
        // the torsion stress warning must NOT fire at exactly the allowable.
        assert!(
            !has_message(&status, "load point 1 stress"),
            "stress exactly at allowable must not warn (strict >)"
        );
    }

    #[test]
    fn at_solid_stress_exactly_at_set_allowable_produces_no_warning() {
        // L0 chosen so that the at_solid force produces stress == allowable_pct_set
        // (0.60 for Music Wire) exactly in IEEE 754.
        // Formula: L0 = Ls + [pct_set * MTS * π * d³ / (K * 8 * D)] / k.
        // Kills `> → ==`, `> → <`, and `> → >=` mutants at line 354.
        use std::f64::consts::PI;
        let d_mm = 2.0_f64;
        let d = d_mm / 1000.0;
        let dm = 20.0_f64 / 1000.0;
        let na = 10.0_f64;
        let c = dm / d;
        let k_berg = (4.0 * c + 2.0) / (4.0 * c - 3.0);
        let g = 80.0_f64 * 1e9;
        let k_spring = g * d.powi(4) / (8.0 * dm.powi(3) * na);
        let mts_pa = 2211.0_f64 / d_mm.powf(0.145) * 1e6;
        let pct_set = 0.60_f64;
        // F_solid that gives pct_set exactly
        let f_solid_target = pct_set * mts_pa * PI * d.powi(3) / (k_berg * 8.0 * dm);
        // SquaredGround: Nt = Na + 2, Ls = d * Nt
        let nt = na + 2.0;
        let ls = d * nt;
        let l0 = ls + f_solid_target / k_spring;
        let m_at_solid = AssemblyMember {
            free_length: Length::from_meters(l0),
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![m_at_solid], &[1.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            !has_message(&status, "stress at solid"),
            "at-solid stress exactly at set allowable must not warn (strict >)"
        );
    }

    #[test]
    fn at_solid_stress_above_set_allowable_generates_warning() {
        // Confirms that > allowable DOES warn; together with the exactly-at test,
        // kills `> → ==`, `> → <`, and `> → >=` at line 354.
        // d=2mm, D=20mm, Na=2, L0=30mm: k=10000 N/m, F_solid=220N,
        // stress_solid≈1590MPa = 79.5% MTS (>> 60% set allowable).
        let stiff = AssemblyMember {
            active_coils: 2.0,
            free_length: Length::from_millimeters(30.0),
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![stiff], &[1.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            has_message(&status, "member 1: stress at solid"),
            "at-solid stress above allowable must warn"
        );
    }

    #[test]
    fn stable_member_produces_no_buckling_warning() {
        // Baseline (L0=60mm, D=20mm, FixedFixed) is well within the buckling
        // stability limit (~103 mm). No buckling warning must appear.
        // Kills the `delete !` mutant in the buckling check (line 366).
        let asm = solve(Topology::Nested, vec![baseline_member()], &[10.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            !has_message(&status, "buckling"),
            "stable member must not produce a buckling warning"
        );
    }

    #[test]
    fn unstable_member_produces_buckling_warning() {
        // Long spring (L0=200mm, D=20mm, FixedFixed): critical L0 ≈ 103mm → unstable.
        // Together with the stable test, pins the `!` semantics.
        let long = AssemblyMember {
            free_length: Length::from_millimeters(200.0),
            ..baseline_member()
        };
        let asm = solve(Topology::Nested, vec![long], &[10.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            has_message(
                &status,
                "member 1: free length exceeds the absolute-stability limit"
            ),
            "unstable member must produce a buckling warning"
        );
    }

    #[test]
    fn load_point_stress_clearly_below_allowable_produces_no_warning() {
        // Baseline at 1 N: pct ≈ 0.045% << 45% allowable → no torsion warning.
        // Kills `> → <` mutant at line 340 (< mutant would warn here).
        let asm = solve(Topology::Nested, vec![baseline_member()], &[1.0]).unwrap();
        let status = evaluate_status(&asm, &materials());
        assert!(
            !has_message(&status, "load point 1 stress"),
            "stress well below allowable must not warn"
        );
    }

    #[test]
    fn member_error_preserves_structure_for_relocalization() {
        // A member DiameterOutOfRange must arrive as Member{ DiameterOutOfRange },
        // not a flattened InconsistentInputs — the GUI relies on the structure.
        let fat = AssemblyMember {
            wire_dia: Length::from_millimeters(10.0),
            mean_dia: Length::from_millimeters(80.0),
            free_length: Length::from_millimeters(200.0),
            ..baseline_member()
        };
        let err = solve(Topology::Series, vec![fat], &[10.0]).unwrap_err();
        assert!(matches!(
            err,
            crate::SpringError::Member { index: 0, ref source }
                if matches!(**source, crate::SpringError::DiameterOutOfRange { .. })
        ));
    }

    #[test]
    fn nested_rate_sum_overflow_trips_the_assembly_guard() {
        // Each member individually solves Ok (finite rate near f64::MAX); the
        // Nested sum k_total = k1 + k2 overflows to +inf — the ASSEMBLY-level
        // guard must fire with its own distinct message.  The member-level guard
        // in design.rs cannot catch this; see huge_load_trips_the_output_guard
        // for the member-level path.
        //
        // Fixture derivation (k = G·d⁴ / (8·D³·Na)):
        //   G=80 GPa, d=2 mm, D=20 mm — Music Wire geometry, in the material's
        //   valid diameter range [0.10, 6.5] mm.
        //   Na chosen so k ≈ 0.6·f64::MAX (finite, below f64::MAX) by inverting
        //   the rate formula: Na = G·d⁴ / (8·D³·k_target).
        //   free_length = solid_length = d·(Na+2) ≈ 4 mm so the at-solid force
        //   is 0 N (stress = 0 Pa, finite) — the member-level guard clears.
        //   With two such members Nested: k1 + k2 = 2·(0.6·MAX) = 1.2·MAX = +inf.
        let d_m = 0.002_f64; // 2 mm
        let dm_m = 0.020_f64; // 20 mm
        let g = 80.0e9_f64;
        let k_target = f64::MAX * 0.6;
        let na = g * d_m.powi(4) / (8.0 * dm_m.powi(3) * k_target);
        // Sanity: Na is a normal, positive, finite f64 — passes solve_forward's guard.
        assert!(
            na.is_finite() && na > 0.0,
            "fixture: Na must be positive-finite"
        );
        // Sanity: individual rate is finite and below f64::MAX.
        let k_individual = g * d_m.powi(4) / (8.0 * dm_m.powi(3) * na);
        assert!(
            k_individual.is_finite() && k_individual < f64::MAX,
            "fixture: individual rate must be finite"
        );
        // Sanity: the Nested sum overflows to +inf.
        assert!(
            (k_individual + k_individual).is_infinite(),
            "fixture: rate sum must overflow to +inf"
        );
        // solid_length (SquaredGround): d·(Na+2) ≈ 4 mm; L0 == Ls so at-solid
        // force = 0 N and its stress = 0 Pa — the member-level guard stays silent.
        let ls_m = d_m * (na + 2.0);
        let huge = AssemblyMember {
            material_name: "Music Wire".to_string(),
            wire_dia: Length::from_meters(d_m),
            mean_dia: Length::from_meters(dm_m),
            active_coils: na,
            free_length: Length::from_meters(ls_m), // L0 == Ls: at-solid travel = 0
            end_type: EndType::SquaredGround,
        };
        // One such member alone must solve Ok — this pins that the overflow is
        // produced solely by the SUM, not by any single-member non-finiteness.
        assert!(
            solve(Topology::Nested, vec![huge.clone()], &[0.0]).is_ok(),
            "fixture: a single huge member must solve Ok"
        );
        // Two members: the Nested sum overflows → assembly guard fires.
        let m = msg(solve(Topology::Nested, vec![huge.clone(), huge], &[0.0]));
        assert_eq!(
            m,
            "assembly solve produced a non-finite result (inputs exceed the representable range)"
        );
    }
}
