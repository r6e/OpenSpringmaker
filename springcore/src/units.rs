//! Strongly-typed physical quantities. Each wraps an `f64` stored in SI base
//! units (metres, newtons, pascals, ...), with one documented exception:
//! `Temperature` is an informational-only quantity stored in degrees Celsius —
//! the engineering convention for material service temperatures — not the SI
//! base unit kelvin. Conversion factors are exact per NIST Special Publication 811.

use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

/// Exact unit-conversion constants (NIST SP 811).
const METERS_PER_INCH: f64 = 0.0254;
const NEWTONS_PER_LBF: f64 = 4.4482216152605;
const PASCALS_PER_PSI: f64 = 6894.757293168;
// 1 lb/in³ = (1 lbm = 0.45359237 kg) / (1 in³ = 0.0254³ m³)
//          = 0.45359237 / 0.0254³ kg/m³  (exact, per NIST SP 811).
const KG_PER_M3_PER_LB_PER_IN3: f64 = 0.45359237 / (0.0254 * 0.0254 * 0.0254);

macro_rules! si_quantity {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(f64);
    };
}

si_quantity!(
    /// Length, stored in metres.
    Length
);
si_quantity!(
    /// Force, stored in newtons.
    Force
);
si_quantity!(
    /// Stress / pressure / elastic modulus, stored in pascals.
    Stress
);
si_quantity!(
    /// Spring rate (force per length), stored in newtons per metre.
    SpringRate
);
si_quantity!(
    /// Frequency, stored in hertz.
    Frequency
);
si_quantity!(
    /// Mass density, stored in kilograms per cubic metre.
    MassDensity
);
si_quantity!(
    /// Temperature, stored in degrees Celsius. Informational only — not used in
    /// any spring calculation.
    Temperature
);
si_quantity!(
    /// Bending/torsional moment (torque), stored in newton-metres.
    Moment
);
si_quantity!(
    /// Angle, stored in radians (SI).
    Angle
);
si_quantity!(
    /// Angular spring rate (moment per angle), stored in newton-metres per radian.
    AngularRate
);

impl Length {
    /// Construct from metres (SI base unit).
    pub fn from_meters(v: f64) -> Self {
        Self(v)
    }
    /// Construct from millimetres (1 mm = 0.001 m).
    pub fn from_millimeters(v: f64) -> Self {
        Self(v / 1000.0)
    }
    /// Construct from inches (1 in = 0.0254 m, NIST SP 811).
    pub fn from_inches(v: f64) -> Self {
        Self(v * METERS_PER_INCH)
    }
    /// Return value in metres.
    pub fn meters(self) -> f64 {
        self.0
    }
    /// Return value in millimetres.
    pub fn millimeters(self) -> f64 {
        self.0 * 1000.0
    }
    /// Return value in inches.
    pub fn inches(self) -> f64 {
        self.0 / METERS_PER_INCH
    }
}

impl Force {
    /// Construct from newtons (SI base unit).
    pub fn from_newtons(v: f64) -> Self {
        Self(v)
    }
    /// Construct from pounds-force (1 lbf = 4.4482216152605 N, NIST SP 811).
    pub fn from_pounds_force(v: f64) -> Self {
        Self(v * NEWTONS_PER_LBF)
    }
    /// Return value in newtons.
    pub fn newtons(self) -> f64 {
        self.0
    }
    /// Return value in pounds-force.
    pub fn pounds_force(self) -> f64 {
        self.0 / NEWTONS_PER_LBF
    }
}

impl Stress {
    /// Construct from pascals (SI base unit).
    pub fn from_pascals(v: f64) -> Self {
        Self(v)
    }
    /// Construct from megapascals (1 MPa = 1e6 Pa).
    pub fn from_megapascals(v: f64) -> Self {
        Self(v * 1.0e6)
    }
    /// Construct from pounds per square inch (1 psi = 6894.757293168 Pa, NIST SP 811).
    pub fn from_psi(v: f64) -> Self {
        Self(v * PASCALS_PER_PSI)
    }
    /// Return value in pascals.
    pub fn pascals(self) -> f64 {
        self.0
    }
    /// Return value in megapascals.
    pub fn megapascals(self) -> f64 {
        self.0 / 1.0e6
    }
    /// Return value in pounds per square inch.
    pub fn psi(self) -> f64 {
        self.0 / PASCALS_PER_PSI
    }
}

impl SpringRate {
    /// Construct from newtons per metre (SI base unit).
    pub fn from_newtons_per_meter(v: f64) -> Self {
        Self(v)
    }
    /// Construct from pounds-force per inch (1 lbf/in = 4.4482216152605/0.0254 N/m).
    pub fn from_pounds_per_inch(v: f64) -> Self {
        Self(v * NEWTONS_PER_LBF / METERS_PER_INCH)
    }
    /// Return value in newtons per metre.
    pub fn newtons_per_meter(self) -> f64 {
        self.0
    }
    /// Return value in pounds-force per inch.
    pub fn pounds_per_inch(self) -> f64 {
        self.0 * METERS_PER_INCH / NEWTONS_PER_LBF
    }
}

impl Moment {
    /// Construct from newton-metres (SI base unit).
    pub fn from_newton_meters(v: f64) -> Self {
        Self(v)
    }
    /// Construct from newton-millimetres (1 N·mm = 0.001 N·m).
    pub fn from_newton_millimeters(v: f64) -> Self {
        Self(v / 1000.0)
    }
    /// Construct from pound-force inches (1 lbf·in = 4.4482216152605 N × 0.0254 m).
    pub fn from_pound_force_inches(v: f64) -> Self {
        Self(v * NEWTONS_PER_LBF * METERS_PER_INCH)
    }
    /// Return value in newton-metres.
    pub fn newton_meters(self) -> f64 {
        self.0
    }
    /// Return value in newton-millimetres.
    pub fn newton_millimeters(self) -> f64 {
        self.0 * 1000.0
    }
    /// Return value in pound-force inches.
    pub fn pound_force_inches(self) -> f64 {
        self.0 / (NEWTONS_PER_LBF * METERS_PER_INCH)
    }
}

impl Angle {
    /// Construct from radians (SI base unit).
    pub fn from_radians(v: f64) -> Self {
        Self(v)
    }
    /// Construct from degrees (1 deg = π/180 rad).
    pub fn from_degrees(v: f64) -> Self {
        Self(v * PI / 180.0)
    }
    /// Construct from turns / revolutions (1 turn = 2π rad).
    pub fn from_turns(v: f64) -> Self {
        Self(v * TAU)
    }
    /// Return value in radians.
    pub fn radians(self) -> f64 {
        self.0
    }
    /// Return value in degrees.
    pub fn degrees(self) -> f64 {
        self.0 * 180.0 / PI
    }
    /// Return value in turns / revolutions.
    pub fn turns(self) -> f64 {
        self.0 / TAU
    }
}

impl AngularRate {
    /// Construct from newton-metres per radian (SI base unit).
    pub fn from_newton_meters_per_radian(v: f64) -> Self {
        Self(v)
    }
    /// Construct from newton-metres per degree (1 N·m/deg = 180/π N·m/rad).
    pub fn from_newton_meters_per_degree(v: f64) -> Self {
        Self(v * 180.0 / PI)
    }
    /// Construct from newton-metres per turn (1 N·m/turn = 1/2π N·m/rad).
    pub fn from_newton_meters_per_turn(v: f64) -> Self {
        Self(v / TAU)
    }
    /// Return value in newton-metres per radian.
    pub fn newton_meters_per_radian(self) -> f64 {
        self.0
    }
    /// Return value in newton-metres per degree.
    pub fn newton_meters_per_degree(self) -> f64 {
        self.0 * PI / 180.0
    }
    /// Return value in newton-metres per turn.
    pub fn newton_meters_per_turn(self) -> f64 {
        self.0 * TAU
    }
}

impl Frequency {
    /// Construct from hertz (SI base unit).
    pub fn from_hertz(v: f64) -> Self {
        Self(v)
    }
    /// Return value in hertz.
    pub fn hertz(self) -> f64 {
        self.0
    }
}

impl MassDensity {
    /// Construct from kilograms per cubic metre (SI base unit).
    pub fn from_kg_per_m3(v: f64) -> Self {
        Self(v)
    }
    /// Construct from pounds-mass per cubic inch (1 lb/in³ = 27679.905 kg/m³).
    pub fn from_pounds_per_in3(v: f64) -> Self {
        Self(v * KG_PER_M3_PER_LB_PER_IN3)
    }
    /// Return value in kilograms per cubic metre.
    pub fn kg_per_m3(self) -> f64 {
        self.0
    }
}

impl Temperature {
    /// Construct from degrees Celsius (the unit this informational quantity is
    /// stored in; see the module header).
    pub fn from_celsius(v: f64) -> Self {
        Self(v)
    }
    /// Construct from degrees Fahrenheit (°C = (°F - 32) × 5/9).
    pub fn from_fahrenheit(v: f64) -> Self {
        Self((v - 32.0) * 5.0 / 9.0)
    }
    /// Return value in degrees Celsius.
    pub fn celsius(self) -> f64 {
        self.0
    }
    /// Return value in degrees Fahrenheit (°F = °C × 9/5 + 32).
    pub fn fahrenheit(self) -> f64 {
        self.0 * 9.0 / 5.0 + 32.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    // Conversion factors per NIST SP 811 (exact): 1 in = 0.0254 m,
    // 1 lbf = 4.4482216152605 N, 1 psi = 6894.757293168 Pa.
    #[test]
    fn length_inch_roundtrip() {
        let l = Length::from_inches(1.0);
        assert_relative_eq!(l.meters(), 0.0254, max_relative = 1e-12);
        assert_relative_eq!(l.inches(), 1.0, max_relative = 1e-12);
        assert_relative_eq!(l.millimeters(), 25.4, max_relative = 1e-12);
    }

    #[test]
    fn force_pound_roundtrip() {
        let f = Force::from_pounds_force(1.0);
        assert_relative_eq!(f.newtons(), 4.4482216152605, max_relative = 1e-12);
        assert_relative_eq!(f.pounds_force(), 1.0, max_relative = 1e-12);
    }

    // Extra non-unity value: kills the "replace pounds_force -> 1.0" mutant.
    #[test]
    fn force_pound_non_unity_roundtrip() {
        // 3 lbf != 1.0, so a mutant returning constant 1.0 would fail here.
        let f = Force::from_pounds_force(3.0);
        assert_relative_eq!(f.pounds_force(), 3.0, max_relative = 1e-12);
        assert_relative_eq!(f.newtons(), 3.0 * 4.4482216152605, max_relative = 1e-12);
    }

    #[test]
    fn stress_psi_roundtrip() {
        let s = Stress::from_psi(1.0);
        assert_relative_eq!(s.pascals(), 6894.757293168, max_relative = 1e-12);
        assert_relative_eq!(s.psi(), 1.0, max_relative = 1e-12);
        assert_relative_eq!(
            Stress::from_megapascals(1.0).pascals(),
            1.0e6,
            max_relative = 1e-12
        );
    }

    #[test]
    fn rate_pounds_per_inch_roundtrip() {
        // 1 lbf/in = 4.4482216152605 / 0.0254 N/m = 175.126835... N/m
        let k = SpringRate::from_pounds_per_inch(1.0);
        assert_relative_eq!(
            k.newtons_per_meter(),
            4.4482216152605 / 0.0254,
            max_relative = 1e-12
        );
        assert_relative_eq!(k.pounds_per_inch(), 1.0, max_relative = 1e-12);
    }

    // Extra non-unity value: kills the "replace pounds_per_inch -> 1.0" mutant.
    #[test]
    fn rate_pounds_per_inch_non_unity_roundtrip() {
        // 5 lbf/in != 1.0, so a mutant returning constant 1.0 would fail here.
        let k = SpringRate::from_pounds_per_inch(5.0);
        assert_relative_eq!(k.pounds_per_inch(), 5.0, max_relative = 1e-12);
        assert_relative_eq!(
            k.newtons_per_meter(),
            5.0 * 4.4482216152605 / 0.0254,
            max_relative = 1e-12
        );
    }

    #[test]
    fn density_pound_per_in3_roundtrip() {
        // 1 lb/in^3 = 27679.9047 kg/m^3 (derived from lbm and inch definitions)
        let d = MassDensity::from_pounds_per_in3(1.0);
        assert_relative_eq!(d.kg_per_m3(), 27679.904710203, max_relative = 1e-9);
    }

    #[test]
    fn temperature_celsius_fahrenheit_roundtrip() {
        let t = Temperature::from_celsius(100.0);
        assert_relative_eq!(t.celsius(), 100.0, max_relative = 1e-12);
        assert_relative_eq!(t.fahrenheit(), 212.0, max_relative = 1e-12);
        let f = Temperature::from_fahrenheit(32.0);
        assert_relative_eq!(f.celsius(), 0.0, max_relative = 1e-12);
        // Boiling point pins the 5/9 scale factor (a zero input cannot:
        // (32-32)*k = 0 for any k). 212 °F = 100 °C.
        let boiling = Temperature::from_fahrenheit(212.0);
        assert_relative_eq!(boiling.celsius(), 100.0, max_relative = 1e-12);
    }

    #[test]
    fn moment_conversions_round_trip() {
        let m = Moment::from_newton_meters(2.0);
        assert_relative_eq!(m.newton_meters(), 2.0, max_relative = 1e-12);
        assert_relative_eq!(m.newton_millimeters(), 2000.0, max_relative = 1e-12);
        // 1 lbf·in = 4.4482216152605 N × 0.0254 m = 0.112984829... N·m
        let one_lbf_in = Moment::from_pound_force_inches(1.0);
        assert_relative_eq!(
            one_lbf_in.newton_meters(),
            0.1129848290276167,
            max_relative = 1e-12
        );
        assert_relative_eq!(one_lbf_in.pound_force_inches(), 1.0, max_relative = 1e-12);
    }

    #[test]
    fn angle_conversions_round_trip() {
        use std::f64::consts::{PI, TAU};
        let a = Angle::from_degrees(180.0);
        assert_relative_eq!(a.radians(), PI, max_relative = 1e-12);
        assert_relative_eq!(a.turns(), 0.5, max_relative = 1e-12);
        let one_turn = Angle::from_turns(1.0);
        assert_relative_eq!(one_turn.radians(), TAU, max_relative = 1e-12);
        assert_relative_eq!(one_turn.degrees(), 360.0, max_relative = 1e-12);
    }

    #[test]
    fn angular_rate_conversions_round_trip() {
        use std::f64::consts::{PI, TAU};
        // 1 N·m/rad → per degree = ×(π/180); per turn = ×2π.
        let k = AngularRate::from_newton_meters_per_radian(1.0);
        assert_relative_eq!(
            k.newton_meters_per_degree(),
            PI / 180.0,
            max_relative = 1e-12
        );
        assert_relative_eq!(k.newton_meters_per_turn(), TAU, max_relative = 1e-12);
        let per_turn = AngularRate::from_newton_meters_per_turn(TAU);
        assert_relative_eq!(
            per_turn.newton_meters_per_radian(),
            1.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn moment_from_newton_millimeters_converts_correctly() {
        // Kills: `/ → %` and `/ → *` mutants on the 1/1000 scale factor (line 166).
        // 1000 N·mm = 1 N·m; 2500 N·mm = 2.5 N·m.
        assert_relative_eq!(
            Moment::from_newton_millimeters(1000.0).newton_meters(),
            1.0,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            Moment::from_newton_millimeters(2500.0).newton_meters(),
            2.5,
            max_relative = 1e-12
        );
    }

    #[test]
    fn moment_pound_force_inches_roundtrip_at_non_unit_value() {
        // Kills: `pound_force_inches → 1.0` constant mutant (line 182) — the existing test
        // only asserts `from_pound_force_inches(1.0).pound_force_inches() == 1.0`, which the
        // "return 1.0" mutant also satisfies.
        assert_relative_eq!(
            Moment::from_pound_force_inches(2.0).pound_force_inches(),
            2.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn angular_rate_from_newton_meters_per_degree_converts_correctly() {
        use std::f64::consts::PI;
        // Kills: `* → +`, `* → /`, `/ → %`, `/ → *` mutants in the 180/π factor (line 220).
        // 1 N·m/deg = 180/π N·m/rad.
        let k = AngularRate::from_newton_meters_per_degree(1.0);
        assert_relative_eq!(
            k.newton_meters_per_radian(),
            180.0 / PI,
            max_relative = 1e-12
        );
        // Round-trip: π/180 N·m/deg = 1 N·m/rad.
        let k2 = AngularRate::from_newton_meters_per_degree(PI / 180.0);
        assert_relative_eq!(k2.newton_meters_per_radian(), 1.0, max_relative = 1e-12);
    }
}
