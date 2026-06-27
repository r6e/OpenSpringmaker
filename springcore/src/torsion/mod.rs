//! Helical torsion springs (round wire). Parallel to the compression and extension
//! families; loaded by a moment M, deflecting through an angle θ, stressed in bending.
//! Reuses `units`, `material`, `numeric`, and the crate-root `DesignStatus`. Formula
//! sources cited at each call site (Shigley Ch. 10; EN 13906-3).

mod design;
mod mechanics;
mod scenario;

pub use design::{solve_forward, TorsionDesign, TorsionInputs, TorsionLoadPoint};
pub use mechanics::FrictionModel;
pub use scenario::{PowerUser, Scenario};
