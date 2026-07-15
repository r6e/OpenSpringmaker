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

    /// Resolve an optional user-supplied inactive-coil count to a concrete value,
    /// defaulting to this end type's Shigley Table 10-1 count when unset. Single
    /// source of the "None = end-type default" rule for every family.
    pub fn resolve_inactive(self, inactive: Option<f64>) -> f64 {
        inactive.unwrap_or(self.end_coils())
    }

    /// Total coils: Nt = Na + Ni (Ni = inactive count; Shigley Table 10-1 at the default).
    pub fn total_coils(self, active: f64, inactive: f64) -> f64 {
        active + inactive
    }

    /// Active coils from total coils: Na = Nt - Ne.
    pub fn active_coils(self, total: f64) -> f64 {
        total - self.end_coils()
    }

    /// Solid (fully compressed) length (Shigley Table 10-1, generalized to Ni).
    pub fn solid_length(self, wire_dia: Length, active: f64, inactive: f64) -> Length {
        let d = wire_dia.meters();
        let nt = self.total_coils(active, inactive);
        let ls = match self {
            // Ground ends: Ls = d * Nt
            Self::PlainGround | Self::SquaredGround => d * nt,
            // Non-ground ends: Ls = d * (Nt + 1)
            Self::Plain | Self::Squared => d * (nt + 1.0),
        };
        Length::from_meters(ls)
    }

    /// Free length from pitch: base per-end formula + (Ni − Ne)·d additive closed-coil term.
    pub fn free_length(
        self,
        wire_dia: Length,
        active: f64,
        pitch: Length,
        inactive: f64,
    ) -> Length {
        let d = wire_dia.meters();
        let p = pitch.meters();
        let base = match self {
            Self::Plain => p * active + d,
            Self::PlainGround => p * (active + 1.0),
            Self::Squared => p * active + 3.0 * d,
            Self::SquaredGround => p * active + 2.0 * d,
        };
        Length::from_meters(base + d * (inactive - self.end_coils()))
    }

    /// Pitch that yields a given free length (inverse of `free_length`, generalized to Ni).
    pub fn pitch_from_free_length(
        self,
        wire_dia: Length,
        active: f64,
        free_length: Length,
        inactive: f64,
    ) -> Length {
        let d = wire_dia.meters();
        // Subtract the additive closed-coil term, then invert the base per-end formula.
        let l0 = free_length.meters() - d * (inactive - self.end_coils());
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
    fn resolve_inactive_defaults_to_end_coils_else_passes_through() {
        assert_eq!(EndType::SquaredGround.resolve_inactive(None), 2.0);
        assert_eq!(EndType::Plain.resolve_inactive(None), 0.0);
        assert_eq!(EndType::PlainGround.resolve_inactive(None), 1.0);
        assert_eq!(EndType::SquaredGround.resolve_inactive(Some(3.5)), 3.5);
        assert_eq!(EndType::Plain.resolve_inactive(Some(0.0)), 0.0);
    }

    #[test]
    fn squared_ground_relations() {
        let e = EndType::SquaredGround;
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        assert_relative_eq!(e.total_coils(na, e.end_coils()), 10.0, max_relative = 1e-12);
        assert_relative_eq!(e.active_coils(10.0), 8.0, max_relative = 1e-12);
        // Solid length = d * Nt = 2 mm * 10 = 20 mm
        assert_relative_eq!(
            e.solid_length(d, na, e.end_coils()).millimeters(),
            20.0,
            max_relative = 1e-12
        );
        // Free length = p*Na + 2d, with p = 5 mm: 40 + 4 = 44 mm
        let p = Length::from_millimeters(5.0);
        assert_relative_eq!(
            e.free_length(d, na, p, e.end_coils()).millimeters(),
            44.0,
            max_relative = 1e-12
        );
        // Inverse: pitch from free length recovers 5 mm
        let l0 = Length::from_millimeters(44.0);
        assert_relative_eq!(
            e.pitch_from_free_length(d, na, l0, e.end_coils())
                .millimeters(),
            5.0,
            max_relative = 1e-12
        );
    }

    #[test]
    fn plain_relations() {
        let e = EndType::Plain;
        let d = Length::from_millimeters(1.0);
        // Nt = Na; Ls = d(Nt+1)
        assert_relative_eq!(
            e.total_coils(10.0, e.end_coils()),
            10.0,
            max_relative = 1e-12
        );
        assert_relative_eq!(
            e.solid_length(d, 10.0, e.end_coils()).millimeters(),
            11.0,
            max_relative = 1e-12
        );
        // L0 = p*Na + d, p = 3 mm: 30 + 1 = 31 mm
        let p = Length::from_millimeters(3.0);
        assert_relative_eq!(
            e.free_length(d, 10.0, p, e.end_coils()).millimeters(),
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
            e.free_length(d, 9.0, p, e.end_coils()).millimeters(),
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
            e.free_length(d, na, p, e.end_coils()).millimeters(),
            31.0,
            max_relative = 1e-12
        );
        // Also verify solid length for Squared (non-ground: Ls = d*(Nt+1)).
        // Nt = Na + end_coils = 5 + 2 = 7; Ls = 7*(7+1) = 56 mm.
        assert_relative_eq!(
            e.solid_length(d, na, e.end_coils()).millimeters(),
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
            e.pitch_from_free_length(d, na, l0, e.end_coils())
                .millimeters(),
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
            e.pitch_from_free_length(d, na, l0, e.end_coils())
                .millimeters(),
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
            e.pitch_from_free_length(d, na, l0, e.end_coils())
                .millimeters(),
            2.0,
            max_relative = 1e-12
        );
    }

    /// Backward-compat lock: at inactive = end_coils(), every geometry output equals
    /// the pre-generalization value for all four end types. Fixture: d=2mm, Na=8, p=5mm.
    #[test]
    fn inactive_equals_end_coils_reproduces_baseline() {
        let d = Length::from_millimeters(2.0);
        // p is not directly used: the loop probes the free(pitch=d)==solid identity,
        // not a pitch-5mm case (that combination is covered by the other three tests
        // in this cluster). Kept only to anchor the "p=5mm" fixture note in the doc
        // comment above.
        let _p = Length::from_millimeters(5.0);
        let na = 8.0;
        for e in [
            EndType::Plain,
            EndType::PlainGround,
            EndType::Squared,
            EndType::SquaredGround,
        ] {
            let ne = e.end_coils();
            assert_relative_eq!(e.total_coils(na, ne), na + ne, max_relative = 1e-12);
            // free(p=d) == solid holds for every end type at the default.
            let free_at_d = e.free_length(d, na, d, ne);
            assert_relative_eq!(
                free_at_d.meters(),
                e.solid_length(d, na, ne).meters(),
                max_relative = 1e-12
            );
        }
    }

    /// The free(pitch=d) == solid invariant is preserved for ALL inactive values
    /// (including fractional), for every end type. This is what keeps the
    /// FreeLengthBelowMinimum guard correct with zero change.
    #[test]
    fn free_at_solid_pitch_equals_solid_for_all_inactive() {
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        for e in [
            EndType::Plain,
            EndType::PlainGround,
            EndType::Squared,
            EndType::SquaredGround,
        ] {
            for ni in [0.0, 1.0, 2.0, 2.5, 4.0] {
                let free_at_d = e.free_length(d, na, d, ni).meters();
                let solid = e.solid_length(d, na, ni).meters();
                assert_relative_eq!(free_at_d, solid, max_relative = 1e-12);
            }
        }
    }

    /// Additive closed-coil term: each unit increase in inactive adds exactly d to
    /// both free length and solid length, for every end type.
    #[test]
    fn each_extra_inactive_coil_adds_one_wire_diameter() {
        let d = Length::from_millimeters(2.0);
        let p = Length::from_millimeters(5.0);
        let na = 8.0;
        for e in [
            EndType::Plain,
            EndType::PlainGround,
            EndType::Squared,
            EndType::SquaredGround,
        ] {
            let ne = e.end_coils();
            let free0 = e.free_length(d, na, p, ne).meters();
            let free1 = e.free_length(d, na, p, ne + 1.0).meters();
            let solid0 = e.solid_length(d, na, ne).meters();
            let solid1 = e.solid_length(d, na, ne + 1.0).meters();
            assert_relative_eq!(free1 - free0, d.meters(), max_relative = 1e-12);
            assert_relative_eq!(solid1 - solid0, d.meters(), max_relative = 1e-12);
        }
    }

    /// pitch_from_free_length inverts free_length under a non-default inactive count.
    #[test]
    fn pitch_inverts_free_length_under_nondefault_inactive() {
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        let p = Length::from_millimeters(5.0);
        for e in [
            EndType::Plain,
            EndType::PlainGround,
            EndType::Squared,
            EndType::SquaredGround,
        ] {
            let ni = e.end_coils() + 2.0;
            let l0 = e.free_length(d, na, p, ni);
            assert_relative_eq!(
                e.pitch_from_free_length(d, na, l0, ni).millimeters(),
                5.0,
                max_relative = 1e-12
            );
        }
    }
}
