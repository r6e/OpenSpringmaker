//! Helical extension springs (round wire). Parallel to the compression family;
//! reuses `units`, `material`, and the identical `mechanics::spring_rate` /
//! `corrected_shear_stress`. Formula sources cited at each call site.

mod design;
mod ends;
mod mechanics;
mod optimize;
mod scenario;

pub use design::{ExtLoadPoint, ExtensionDesign};
pub use ends::HookEnds;
pub use mechanics::free_length_from_geometry;
pub use optimize::{
    solve_min_weight, ExtBindingConstraint, ExtMinWeightRequest, ExtMinWeightSolution, HookSpec,
};
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
