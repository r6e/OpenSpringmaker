//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

pub mod end_type;
pub mod error;
pub mod material;
pub mod mechanics;
pub mod numeric;
pub mod units;

pub use end_type::EndType;
pub use error::{Result, SpringError};
pub use material::{Endurance, Material, MaterialSet, MtsEquation, MtsForm, StrengthUnits};
pub use mechanics::EndFixity;
pub use numeric::{find_root_bracketed, SolveConfig};
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
