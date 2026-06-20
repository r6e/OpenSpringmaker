//! Strongly-typed physical quantities. Each wraps an `f64` stored in SI base
//! units. Conversion factors are exact per NIST Special Publication 811.

use serde::{Deserialize, Serialize};

/// Exact unit-conversion constants (NIST SP 811).
const METERS_PER_INCH: f64 = 0.0254;
const NEWTONS_PER_LBF: f64 = 4.4482216152605;
const PASCALS_PER_PSI: f64 = 6894.757293168;
// 1 lb/in^3 = NEWTONS_PER_LBF/g_n converted... derived as mass: 1 lbm = 0.45359237 kg,
// 1 in^3 = 0.0254^3 m^3 -> 0.45359237 / 0.0254^3.
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

    #[test]
    fn density_pound_per_in3_roundtrip() {
        // 1 lb/in^3 = 27679.9047 kg/m^3 (derived from lbm and inch definitions)
        let d = MassDensity::from_pounds_per_in3(1.0);
        assert_relative_eq!(d.kg_per_m3(), 27679.904710203, max_relative = 1e-9);
    }
}
