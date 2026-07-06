//! Conical (tapered) round-wire compression springs — LINEAR-RANGE model.
//!
//! The rate is Shigley Prob. 10-29's Castigliano result over a linearly
//! tapering coil radius; it holds while ALL coils are active. The
//! progressive-rate regime — the largest (most compliant) coil bottoms first
//! and the spring stiffens — is contact-progression behavior with no cited
//! in-house treatment and is OUT OF SCOPE.
//!
//! Deliberate omissions (no cited conical replacements in-house; do not
//! approximate): natural frequency (the cylindrical surge formula assumes a
//! uniform coil), buckling (conical springs are inherently more stable than
//! the cylindrical criterion assumes), fatigue (callable later at the
//! governing large-end coil), telescoped solid height (the reported solid
//! length is the conservative non-telescoping stack; see
//! `ConicalDesign::telescopes`).

mod design;

pub use design::{evaluate_status, solve_forward, ConicalDesign, ConicalInputs};
