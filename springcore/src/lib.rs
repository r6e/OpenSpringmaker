//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

pub(crate) mod design;
pub(crate) mod end_type;
pub mod error;
pub(crate) mod fatigue;
pub(crate) mod material;
pub mod material_store;
pub(crate) mod mechanics;
pub(crate) mod numeric;
pub(crate) mod optimize;
pub(crate) mod persistence;
pub(crate) mod scenario;
pub mod units;

#[cfg(test)]
pub(crate) mod test_support;

pub use design::{
    evaluate_status, solve_forward, DesignStatus, LoadPoint, Severity, SpringDesign, StatusMessage,
};
pub use end_type::EndType;
pub use error::{Result, SpringError};
pub use fatigue::{analyze_fatigue, FatigueResult};
pub use material::{Endurance, Material, MaterialSet};
pub use material_store::MaterialStore;
pub use mechanics::EndFixity;
pub use optimize::{solve_min_weight, BindingConstraint, MinWeightRequest, MinWeightSolution};
pub use persistence::{min_weight_request_from_spec, SavedDesign, ScenarioSpec, UnitSystem};
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress, Temperature};
