//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

pub mod end_type;
pub mod error;
pub mod mechanics;
pub mod units;

pub use end_type::EndType;
pub use error::{Result, SpringError};
pub use mechanics::EndFixity;
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
