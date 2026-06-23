//! Hook/loop end geometry for extension springs.

use crate::units::Length;

/// Mean bend radii of the two hook curvatures: r1 at the hook (point A, bending)
/// and r2 at the side bend (point B, torsion).
#[derive(Debug, Clone, Copy)]
pub struct HookEnds {
    pub r1: Length,
    pub r2: Length,
}

impl HookEnds {
    /// Standard machine-loop defaults: r1 = D/2, r2 = D/4 (spec default).
    pub fn default_for(mean_dia: Length) -> Self {
        Self {
            r1: Length::from_meters(mean_dia.meters() / 2.0),
            r2: Length::from_meters(mean_dia.meters() / 4.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn default_hook_radii_are_half_and_quarter_mean() {
        let h = HookEnds::default_for(Length::from_millimeters(20.0));
        assert_relative_eq!(h.r1.millimeters(), 10.0, max_relative = 1e-12);
        assert_relative_eq!(h.r2.millimeters(), 5.0, max_relative = 1e-12);
    }
}
