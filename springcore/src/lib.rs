//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

pub mod design;
pub mod end_type;
pub mod error;
pub mod fatigue;
pub mod material;
pub mod mechanics;
pub mod numeric;
pub mod optimize;
pub mod persistence;
pub mod scenario;
pub mod units;

pub use design::{
    evaluate_status, solve_forward, DesignStatus, LoadPoint, Severity, SpringDesign, StatusMessage,
};
pub use end_type::EndType;
pub use error::{Result, SpringError};
pub use fatigue::{analyze_fatigue, FatigueResult};
pub use material::{Endurance, Material, MaterialSet, MtsEquation, MtsForm, StrengthUnits};
pub use mechanics::EndFixity;
pub use numeric::{find_root_bracketed, SolveConfig};
pub use optimize::{solve_min_weight, BindingConstraint, MinWeightRequest, MinWeightSolution};
pub use persistence::{SavedDesign, ScenarioSpec, UnitSystem};
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
