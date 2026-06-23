//! Accuracy contract. Part A is self-contained; Part B cross-checks published
//! worked examples (values transcribed from the cited sources).

use approx::assert_relative_eq;
use springcore::units::{Force, Length, MassDensity};
use springcore::{
    analyze_fatigue, evaluate_status, MaterialSet, SavedDesign, Scenario, ScenarioSpec, UnitSystem,
};

#[test]
fn pipeline_rate_based_music_wire() {
    // Clean reference case validated unit-by-unit in the module tests:
    // d=2mm, D=20mm (C=10), G=80 GPa, target rate 2000 N/m -> Na=10.
    let saved = SavedDesign {
        material: "Music Wire".into(),
        unit_system: UnitSystem::Metric,
        scenario: ScenarioSpec::RateBased {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            rate_n_per_m: 2000.0,
            free_length_mm: 60.0,
            loads_n: vec![10.0, 30.0],
        },
    };
    let set = MaterialSet::load_default();
    let design = saved.solve(&set).unwrap();

    assert_relative_eq!(design.index, 10.0, max_relative = 1e-9);
    assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    assert_relative_eq!(design.active_coils, 10.0, max_relative = 1e-6);
    assert_relative_eq!(design.total_coils, 12.0, max_relative = 1e-9);
    assert_relative_eq!(design.solid_length.millimeters(), 24.0, max_relative = 1e-6);
    // 30 N -> 15 mm deflection at 2000 N/m.
    assert_relative_eq!(
        design.load_points[1].deflection.millimeters(),
        15.0,
        max_relative = 1e-6
    );

    // Status: index 10 is in-range, so no index caution.
    let status = evaluate_status(&design, set.get("Music Wire").unwrap());
    assert!(!status.messages.iter().any(|m| m.message.contains("index")));

    // Fatigue over the 10–30 N cycle returns a finite, positive safety factor.
    let fat = analyze_fatigue(
        set.get("Music Wire").unwrap(),
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(10.0),
        Force::from_newtons(30.0),
    )
    .unwrap();
    assert!(fat.goodman_factor_of_safety > 1.0);
}

// Part B — independent published-source cross-check.
//
// This is the engine's independent numerical oracle: inputs and results are
// taken from a third-party worked example, NOT recomputed by us, so it catches
// a wrong constant in the engine that the formula-based unit tests would share.
//
// Source: "Comprehensive Spring Design" handbook procedure, §7.13.1 Calculation
// Examples (a production music-wire compression spring used in ~50,000
// mechanisms), as reproduced publicly by Victory Spring Ltd.:
// https://victoryspring.ca/wp-content/uploads/2021/01/comprehensive-spring-design.pdf
//
// The example (US customary units) specifies a music-wire compression spring
// with closed-and-ground ends:
//   d = 0.250 in, OD = 1.94 in -> mean D = 1.69 in, spring index C = 6.76,
//   N = 13 active coils (15 total), free length 8 in, G = 11.5e6 psi.
// and publishes:
//   spring rate R = 89 lb/in,  Wahl factor Ka = 1.221,
//   load P = 356 lb at 4 in deflection,  corrected shear stress S = 119,695 psi.
//
// Verified internally consistent by hand against the canonical formulas
// (k = G d^4 / 8 N D^3; Ka = (4C-1)/(4C-4) + 0.615/C; S = 8 Ka P D / pi d^3).
// The 3% tolerance absorbs the source's rounding and the small difference
// between its G = 11.5e6 psi and our cited G = 11.6e6 psi (Shigley Table 10-5);
// shear stress is independent of G.
#[test]
fn comprehensive_spring_design_compression() {
    let set = MaterialSet::load_default();
    let material = set.get("Music Wire").unwrap();

    let design = springcore::PowerUser {
        end_type: springcore::EndType::SquaredGround, // closed and ground
        fixity: springcore::EndFixity::FixedFixed,
        wire_dia: Length::from_inches(0.250),
        mean_dia: Length::from_inches(1.69),
        active: 13.0,
        free_length: Length::from_inches(8.0),
        loads: vec![Force::from_pounds_force(356.0)], // P at 4 in deflection
    }
    .solve(material)
    .unwrap();

    // Spring index C = D/d = 1.69 / 0.250 = 6.76 (exact).
    assert_relative_eq!(design.index, 6.76, max_relative = 1e-9);
    // Solid length = d * Nt = 0.250 * 15 = 3.75 in (closed & ground, Nt = Na + 2).
    assert_relative_eq!(design.solid_length.inches(), 3.75, max_relative = 1e-9);
    // Published spring rate R = 89 lb/in (within 3%, absorbing the G-source diff).
    assert_relative_eq!(design.rate.pounds_per_inch(), 89.0, max_relative = 0.03);
    // Published corrected shear stress S = 119,695 psi at P = 356 lb.
    assert_relative_eq!(
        design.load_points[0].shear_stress.psi(),
        119_695.0,
        max_relative = 0.03
    );
}

#[test]
#[ignore = "awaiting EN 13906-1 worked-example source values — RELEASE BLOCKER, see PR body"]
fn en_13906_1_worked_example() {
    // Inputs and expected values transcribed from EN 13906-1 (Cylindrical
    // helical springs — Calculation and design), worked example (metric units).
    //
    // TODO: Locate the EN 13906-1 worked example (appendix or normative body).
    //       Record the given values for d, D, n (active coils), L0, end type,
    //       and operating force, then fill in the zeros below.
    //       Also record the published results for spring rate and shear stress.
    let set = MaterialSet::load_default();

    // GIVEN (from the source — replace each 0.0 with the printed figure):
    let wire_dia = Length::from_millimeters(/* d from EN 13906-1 example */ 0.0);
    let mean_dia = Length::from_millimeters(/* D from EN 13906-1 example */ 0.0);
    let active = /* n from EN 13906-1 example */ 0.0_f64;
    let free_length = Length::from_millimeters(/* L0 from EN 13906-1 example */ 0.0);
    let load = Force::from_newtons(/* F from EN 13906-1 example */ 0.0);

    // Use the material named in the example; update the string if it differs.
    let material = set.get("Music Wire").unwrap();

    let design = springcore::PowerUser {
        end_type: /* end type from EN 13906-1 example */ springcore::EndType::SquaredGround,
        fixity: springcore::EndFixity::FixedFixed,
        wire_dia,
        mean_dia,
        active,
        free_length,
        loads: vec![load],
    }
    .solve(material)
    .unwrap();

    // PUBLISHED RESULTS (from the source — replace each 0.0 with the printed figure):
    assert_relative_eq!(
        design.rate.newtons_per_meter(),
        /* k */ 0.0,
        max_relative = 0.03
    );
    assert_relative_eq!(
        design.solid_length.millimeters(),
        /* Ls */ 0.0,
        max_relative = 0.03
    );
    assert_relative_eq!(
        design.load_points[0].shear_stress.megapascals(),
        /* tau */ 0.0,
        max_relative = 0.03
    );
}

// ── Extension springs ─────────────────────────────────────────────────────────

// Independent published-source cross-check for the extension family. Inputs and
// results are transcribed (not recomputed by us) from Shigley's worked example,
// so a wrong constant in the extension solver that the formula-based unit tests
// would share is caught here.
//
// Source: Budynas & Nisbett, "Shigley's Mechanical Engineering Design", 10th ed.,
// Example 10-6 (A227 hard-drawn steel extension spring). Reproduced publicly in
// the McGraw-Hill "Chapter 10 Mechanical Springs" lecture slides, e.g.
// https://www.point32.com/jenkins_he/documents/SpringsCh10ExtensionTorsionsprings.pdf
//
// The example's hook formulas match our engine's exactly: the bending factor
// (K)_A = (4C1^2 - C1 - 1)/(4 C1 (C1 - 1)) with C1 = 2 r1/d (Eq. 10-35) and the
// torsion factor (K)_B = (4C2 - 1)/(4C2 - 4) with C2 = 2 r2/d (Eq. 10-37). The
// hook stresses sigma_A and tau_B are independent of G, so they assert tightly.
//
// GIVEN (US customary): d = 0.035 in, OD = 0.248 in -> D = 0.213 in, body turns
//   Nb = 12.17, F_i = 1.19 lbf, hook radii r1 = 0.106 in, r2 = 0.089 in; static
//   service load F = 5.25 lbf. Shigley uses Na = Nb + G/E = 12.17 + 11.6/28.7 =
//   12.574 (Eq. 10-40); our PowerUser takes active coils directly, so we pass
//   that Na. L0 = 0.817 in (Eq. 10-39) is supplied as the free length.
// PUBLISHED: k = 17.91 lbf/in, hook bending sigma_A = 156.9 kpsi, hook torsion
//   tau_B = 78.4 kpsi, body shear tau = 82.0 kpsi.
//
// Tolerances: sigma_A / tau_B are G-independent and reproduce to <0.3% (Shigley's
// rounding of the K factors), so 1%. k uses a 3% band absorbing the source's
// G = 11.6e6 psi vs our curated A227 G = 11.5e6 psi (MH Table 20). Body shear
// uses 2% because Shigley applies the Bergsträsser factor (1.234) while our
// static path currently applies the Wahl factor (1.249), ~1.2% higher; this
// band tightens once the selectable curvature-correction model lands.
#[test]
fn extension_shigley_worked_example() {
    use springcore::extension::{HookEnds, PowerUser, Scenario};

    let set = MaterialSet::load_default();
    let material = set.get("Hard-Drawn MB").unwrap(); // A227 hard-drawn steel

    let d = PowerUser {
        wire_dia: Length::from_inches(0.035),
        mean_dia: Length::from_inches(0.213), // OD - d = 0.248 - 0.035
        active: 12.574,                       // Na = Nb + G/E = 12.17 + 11.6/28.7 (Eq. 10-40)
        free_length: Length::from_inches(0.817), // L0 (Eq. 10-39)
        initial_tension: Force::from_pounds_force(1.19),
        hooks: HookEnds {
            r1: Length::from_inches(0.106),
            r2: Length::from_inches(0.089),
        },
        loads: vec![Force::from_pounds_force(5.25)],
    }
    .solve(material)
    .unwrap();

    let lp = &d.load_points[0];
    // Published spring rate k = 17.91 lbf/in (3% absorbs the G-source difference).
    assert_relative_eq!(d.rate.pounds_per_inch(), 17.91, max_relative = 0.03);
    // Hook bending at A: sigma_A = 156.9 kpsi (G-independent).
    assert_relative_eq!(lp.hook_bending.psi(), 156_900.0, max_relative = 0.01);
    // Hook torsion at B: tau_B = 78.4 kpsi (G-independent).
    assert_relative_eq!(lp.hook_torsion.psi(), 78_400.0, max_relative = 0.01);
    // Body torsional shear tau = 82.0 kpsi (2% absorbs Wahl vs Bergsträsser).
    assert_relative_eq!(lp.body_shear.psi(), 82_000.0, max_relative = 0.02);
    // Deflection under the service load: y = (F - F_i)/k = (5.25 - 1.19)/17.91 =
    // 0.227 in — exercises the initial-tension offset (3% tracks the k tolerance).
    assert_relative_eq!(lp.deflection.inches(), 0.227, max_relative = 0.03);
}

// ── PR (b) curated materials: per-material data-correctness goldens ──────────
// Each asserts the strength model reproduces an INDEPENDENT published tensile
// point (Machinery's Handbook 32nd ed., p390 "Minimum Tensile Strength of Spring
// Wire by Diameter"), plus E/G/density spot-checks against MH Table 20 source
// values. E/G are asserted directly in psi and density via the canonical
// `MassDensity::from_pounds_per_in3` constructor, so the test reuses
// springcore::units' NIST conversion factors rather than re-declaring them.
// See docs/superpowers/research/2026-06-21-pr-b-materials-data.md.

#[test]
fn material_hard_drawn_a227() {
    let set = MaterialSet::load_default();
    let m = set.get("Hard-Drawn MB").unwrap();
    // MH p390: 0.080 in -> 227 kpsi = 1565 MPa (diameter from the cited inch value).
    let sut = m.min_tensile_strength(Length::from_inches(0.080)).unwrap();
    assert_relative_eq!(sut.megapascals(), 1565.0, max_relative = 0.02);
    // MH Table 20 (0.064-0.125 in band): E = 28.6 Mpsi, G = 11.5 Mpsi.
    assert_relative_eq!(m.youngs_modulus.psi(), 28.6e6, max_relative = 0.005);
    assert_relative_eq!(m.shear_modulus.psi(), 11.5e6, max_relative = 0.005);
    assert_relative_eq!(
        m.density.kg_per_m3(),
        MassDensity::from_pounds_per_in3(0.284).kg_per_m3(),
        max_relative = 0.005
    );
}

#[test]
fn material_chrome_vanadium_a231() {
    let set = MaterialSet::load_default();
    let m = set.get("Chrome-Vanadium").unwrap();
    // MH p390 "Cr-V Alloy": 0.105 in -> 229 kpsi = 1579 MPa (diameter from the cited inch value).
    let sut = m.min_tensile_strength(Length::from_inches(0.105)).unwrap();
    assert_relative_eq!(sut.megapascals(), 1579.0, max_relative = 0.02);
    // MH Table 20: E = 28.5 Mpsi, G = 11.2 Mpsi.
    assert_relative_eq!(m.youngs_modulus.psi(), 28.5e6, max_relative = 0.005);
    assert_relative_eq!(m.shear_modulus.psi(), 11.2e6, max_relative = 0.005);
    assert_relative_eq!(
        m.density.kg_per_m3(),
        MassDensity::from_pounds_per_in3(0.284).kg_per_m3(),
        max_relative = 0.005
    );
}

#[test]
fn material_phosphor_bronze_b159() {
    let set = MaterialSet::load_default();
    let m = set.get("Phosphor Bronze").unwrap();
    // MH p390 "Phosphor Bronze": 0.041 in -> 135 kpsi = 931 MPa (diameter from the
    // cited inch value), INDEPENDENTLY confirmed by ASTM B159 (135 kpsi, 0.025-0.0625 in band).
    let sut = m.min_tensile_strength(Length::from_inches(0.041)).unwrap();
    assert_relative_eq!(sut.megapascals(), 931.0, max_relative = 0.02);
    // MH Table 20 ("Phosphor Bronze 5 percent tin"): E = 15.0 Mpsi, G = 6.0 Mpsi.
    assert_relative_eq!(m.youngs_modulus.psi(), 15.0e6, max_relative = 0.005);
    assert_relative_eq!(m.shear_modulus.psi(), 6.0e6, max_relative = 0.005);
    assert_relative_eq!(
        m.density.kg_per_m3(),
        MassDensity::from_pounds_per_in3(0.32).kg_per_m3(),
        max_relative = 0.005
    );
}

#[test]
fn curated_set_has_seven_materials() {
    // 4 from sub-project 1 + 3 added in PR (b).
    assert_eq!(MaterialSet::load_default().names().len(), 7);
}
