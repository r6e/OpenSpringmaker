//! Helical torsion springs (round wire). Parallel to the compression and extension
//! families; loaded by a moment M, deflecting through an angle θ, stressed in bending.
//! Reuses `units`, `material`, `numeric`, and the crate-root `DesignStatus`. Formula
//! sources cited at each call site (Shigley Ch. 10; EN 13906-3).

mod mechanics;

pub use mechanics::{
    active_coils_with_legs, angular_rate, bending_stress_inner, bending_stress_nominal,
    kbi_factor, wound_mean_diameter, FrictionModel,
};
