//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

pub mod units;
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
