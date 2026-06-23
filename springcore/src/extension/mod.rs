//! Helical extension springs (round wire). Parallel to the compression family;
//! reuses `units`, `material`, and the identical `mechanics::spring_rate` /
//! `corrected_shear_stress`. Formula sources cited at each call site.

pub mod design;
pub mod ends;
pub mod mechanics;
pub mod scenario;

pub use design::{ExtLoadPoint, ExtensionDesign};
pub use ends::HookEnds;
pub use scenario::{PowerUser, Scenario};
