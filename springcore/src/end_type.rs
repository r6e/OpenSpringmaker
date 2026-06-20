//! End conditions for helical compression springs and the coil/length
//! relations they imply. All relations per Shigley Table 10-1.

use crate::units::Length;
use serde::{Deserialize, Serialize};

/// Spring end condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndType {
    Plain,
    PlainGround,
    Squared,
    SquaredGround,
}

impl EndType {
    /// Number of inactive end coils (Shigley Table 10-1).
    pub fn end_coils(self) -> f64 {
        match self {
            Self::Plain => 0.0,
            Self::PlainGround => 1.0,
            Self::Squared | Self::SquaredGround => 2.0,
        }
    }

    /// Total coils from active coils: Nt = Na + Ne (Shigley Table 10-1).
    pub fn total_coils(self, active: f64) -> f64 {
        active + self.end_coils()
    }

    /// Active coils from total coils: Na = Nt - Ne.
    pub fn active_coils(self, total: f64) -> f64 {
        total - self.end_coils()
    }

    /// Solid (fully compressed) length (Shigley Table 10-1).
    pub fn solid_length(self, wire_dia: Length, active: f64) -> Length {
        let d = wire_dia.meters();
        let nt = self.total_coils(active);
        let ls = match self {
            // Ground ends: Ls = d * Nt
            Self::PlainGround | Self::SquaredGround => d * nt,
            // Non-ground ends: Ls = d * (Nt + 1)
            Self::Plain | Self::Squared => d * (nt + 1.0),
        };
        Length::from_meters(ls)
    }

    /// Free length from pitch (Shigley Table 10-1).
    pub fn free_length(self, wire_dia: Length, active: f64, pitch: Length) -> Length {
        let d = wire_dia.meters();
        let p = pitch.meters();
        let l0 = match self {
            Self::Plain => p * active + d,
            Self::PlainGround => p * (active + 1.0),
            Self::Squared => p * active + 3.0 * d,
            Self::SquaredGround => p * active + 2.0 * d,
        };
        Length::from_meters(l0)
    }

    /// Pitch that yields a given free length (inverse of `free_length`).
    pub fn pitch_from_free_length(
        self,
        wire_dia: Length,
        active: f64,
        free_length: Length,
    ) -> Length {
        let d = wire_dia.meters();
        let l0 = free_length.meters();
        let p = match self {
            Self::Plain => (l0 - d) / active,
            Self::PlainGround => l0 / (active + 1.0),
            Self::Squared => (l0 - 3.0 * d) / active,
            Self::SquaredGround => (l0 - 2.0 * d) / active,
        };
        Length::from_meters(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn squared_ground_relations() {
        let e = EndType::SquaredGround;
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        assert_relative_eq!(e.total_coils(na), 10.0, max_relative = 1e-12);
        assert_relative_eq!(e.active_coils(10.0), 8.0, max_relative = 1e-12);
        // Solid length = d * Nt = 2 mm * 10 = 20 mm
        assert_relative_eq!(
            e.solid_length(d, na).millimeters(),
            20.0,
            max_relative = 1e-12
        );
        // Free length = p*Na + 2d, with p = 5 mm: 40 + 4 = 44 mm
        let p = Length::from_millimeters(5.0);
        assert_relative_eq!(
            e.free_length(d, na, p).millimeters(),
            44.0,
            max_relative = 1e-12
        );
        // Inverse: pitch from free length recovers 5 mm
        let l0 = Length::from_millimeters(44.0);
        assert_relative_eq!(
            e.pitch_from_free_length(d, na, l0).millimeters(),
            5.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn plain_relations() {
        let e = EndType::Plain;
        let d = Length::from_millimeters(1.0);
        // Nt = Na; Ls = d(Nt+1)
        assert_relative_eq!(e.total_coils(10.0), 10.0, max_relative = 1e-12);
        assert_relative_eq!(
            e.solid_length(d, 10.0).millimeters(),
            11.0,
            max_relative = 1e-12
        );
        // L0 = p*Na + d, p = 3 mm: 30 + 1 = 31 mm
        let p = Length::from_millimeters(3.0);
        assert_relative_eq!(
            e.free_length(d, 10.0, p).millimeters(),
            31.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn plain_ground_free_length_uses_na_plus_one() {
        let e = EndType::PlainGround;
        let d = Length::from_millimeters(1.0);
        // L0 = p*(Na+1), p = 2 mm, Na = 9: 2*10 = 20 mm
        let p = Length::from_millimeters(2.0);
        assert_relative_eq!(
            e.free_length(d, 9.0, p).millimeters(),
            20.0,
            max_relative = 1e-12
        );
    }

    // --- Squared free_length (line 56): p * active + 3.0 * d ---
    // Use p=2 mm, Na=5, d=7 mm → 2*5 + 3*7 = 10 + 21 = 31 mm.
    // Every operator mutation (+→-, +→*, *→+, *→/, 3*d→3+d, 3*d→3/d) produces
    // a value clearly distinct from 31, so a single exact assertion kills all six.
    #[test]
    fn squared_free_length_distinguishes_all_operators() {
        let e = EndType::Squared;
        let d = Length::from_millimeters(7.0);
        let p = Length::from_millimeters(2.0);
        let na = 5.0;
        // p*Na + 3*d = 2*5 + 3*7 = 31
        assert_relative_eq!(
            e.free_length(d, na, p).millimeters(),
            31.0,
            max_relative = 1e-12
        );
        // Also verify solid length for Squared (non-ground: Ls = d*(Nt+1)).
        // Nt = Na + end_coils = 5 + 2 = 7; Ls = 7*(7+1) = 56 mm.
        assert_relative_eq!(
            e.solid_length(d, na).millimeters(),
            56.0,
            max_relative = 1e-12
        );
    }

    // --- pitch_from_free_length: all three untested arms ---

    // Plain arm (line 72): (l0 - d) / active
    // l0=16 mm, d=1 mm, Na=5 → (16-1)/5 = 3 mm.
    // -→+: 17/5=3.4; -→/: (16/1)/5=3.2; /→*: 15*5=75; /→%: 15%5=0.
    // Also ensures plain_relations pitch inverse is covered.
    #[test]
    fn plain_pitch_from_free_length() {
        let e = EndType::Plain;
        let d = Length::from_millimeters(1.0);
        let na = 5.0;
        let l0 = Length::from_millimeters(16.0);
        assert_relative_eq!(
            e.pitch_from_free_length(d, na, l0).millimeters(),
            3.0,
            max_relative = 1e-12
        );
    }

    // PlainGround arm (line 73): l0 / (active + 1.0)
    // l0=20 mm, Na=9 → 20/10 = 2 mm.
    // /→%: 20%10=0; /→*: 20*10=200; +→-: 20/8=2.5; +→*: 20/(9*1)≈2.22.
    #[test]
    fn plain_ground_pitch_from_free_length() {
        let e = EndType::PlainGround;
        let d = Length::from_millimeters(1.0); // unused in PlainGround formula but required
        let na = 9.0;
        let l0 = Length::from_millimeters(20.0);
        assert_relative_eq!(
            e.pitch_from_free_length(d, na, l0).millimeters(),
            2.0,
            max_relative = 1e-12
        );
    }

    // Squared arm (line 74): (l0 - 3.0 * d) / active
    // l0=31 mm, d=7 mm, Na=5 → (31-21)/5 = 2 mm.
    // -→+: 52/5=10.4; -→/: (31/21)/5≈0.295; 3*d→3+d: (31-10)/5=4.2;
    // 3*d→3/d: (31-3/7)/5≈6.11; /→*: 10*5=50; /→%: 10%5=0.
    // Also round-trips with squared_free_length_distinguishes_all_operators.
    #[test]
    fn squared_pitch_from_free_length() {
        let e = EndType::Squared;
        let d = Length::from_millimeters(7.0);
        let na = 5.0;
        let l0 = Length::from_millimeters(31.0);
        assert_relative_eq!(
            e.pitch_from_free_length(d, na, l0).millimeters(),
            2.0,
            max_relative = 1e-12
        );
    }
}
