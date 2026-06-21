//! Accuracy contract. Part A is self-contained; Part B cross-checks published
//! worked examples (values transcribed from the cited sources).

use approx::assert_relative_eq;
use springcore::units::{Force, Length};
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

// RELEASE BLOCKER: The two tests below are #[ignore]'d because they require
// numeric values transcribed directly from physical copies of the cited
// engineering references.  Fabricating or web-scraping published test vectors
// is an integrity violation — the numbers MUST be read from the source and
// entered here before these tests are un-ignored.  Until then they remain as
// structural placeholders that compile cleanly but do not execute.

#[test]
#[ignore = "awaiting Shigley 10th ed. Ex 10-1 source values — RELEASE BLOCKER, see PR body"]
fn shigley_example_10_1() {
    // Inputs and expected values transcribed from Shigley's Mechanical
    // Engineering Design, 10th ed., Example 10-1 (helical compression spring,
    // music wire, US customary units).
    //
    // TODO: Open Shigley 10th ed., p. <page TBD>, Example 10-1.
    //       Record the given values for d, D (or OD), Na (or Nt), L0, end
    //       type, and the load F, then fill in the zeros below.
    //       Also record the published results for k, Ls, and tau_corrected.
    let set = MaterialSet::load_default();
    let material = set.get("Music Wire").unwrap();

    // GIVEN (from the source — replace each 0.0 with the printed figure):
    let wire_dia = Length::from_inches(/* d from Ex 10-1 */ 0.0);
    let mean_dia = Length::from_inches(/* D from Ex 10-1 */ 0.0);
    let active = /* Na from Ex 10-1 */ 0.0_f64;
    let free_length = Length::from_inches(/* L0 from Ex 10-1 */ 0.0);
    let load = Force::from_pounds_force(/* F from Ex 10-1 */ 0.0);

    let design = springcore::PowerUser {
        end_type: /* end type from Ex 10-1 */ springcore::EndType::SquaredGround,
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
        design.rate.pounds_per_inch(),
        /* k */ 0.0,
        max_relative = 0.03
    );
    assert_relative_eq!(
        design.solid_length.inches(),
        /* Ls */ 0.0,
        max_relative = 0.03
    );
    assert_relative_eq!(
        design.load_points[0].shear_stress.psi(),
        /* tau */ 0.0,
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
