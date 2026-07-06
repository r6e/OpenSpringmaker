//! Engineering calculations for helical compression and extension springs.
//!
//! Compression springs are modeled by the crate-root API (`PowerUser`,
//! `SpringDesign`, and the scenario types); extension springs — with initial
//! tension and hook stresses — live in the [`extension`] module.
//!
//! All public quantities are stored internally in SI units, with one documented
//! exception: `Temperature` is informational-only and stored in degrees Celsius
//! (the engineering convention for material service temperatures), not the SI
//! base unit kelvin. See the crate `ARCHITECTURE.md` and `docs/adr/` for design
//! rationale.

pub mod conical;
pub(crate) mod design;
pub(crate) mod end_type;
pub mod error;
pub mod extension;
pub mod family;
pub(crate) mod fatigue;
pub(crate) mod material;
pub(crate) mod material_persist;
pub mod material_store;
pub(crate) mod mechanics;
pub(crate) mod numeric;
pub(crate) mod optimize;
pub(crate) mod persistence;
pub(crate) mod scenario;
pub mod torsion;
pub mod units;

#[cfg(test)]
pub(crate) mod test_support;

pub use design::{
    evaluate_status, solve_forward, DesignStatus, LoadPoint, Severity, SpringDesign, StatusMessage,
};
pub use end_type::EndType;
pub use error::{Result, SpringError};
pub use family::{Family, ALL_FAMILIES};
pub use fatigue::{analyze_fatigue, FatigueResult};
pub use material::{
    BendingFatigue, BendingFatigueDraft, Endurance, EnduranceDraft, Material, MaterialDraft,
    MaterialSet, MtsForm, StrengthUnits,
};
pub use material_persist::{user_overlay_path, LoadWarning};
pub use material_store::MaterialStore;
pub use mechanics::{CurvatureCorrection, EndFixity};
pub use optimize::{solve_min_weight, BindingConstraint, MinWeightRequest, MinWeightSolution};
pub use persistence::{
    min_weight_request_from_spec, DesignSpec, ExtScenarioSpec, HookSpecSpec, SavedDesign,
    ScenarioSpec, TorsionSpec, UnitSystem,
};
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
pub use units::{
    Angle, AngularRate, Force, Frequency, Length, MassDensity, Moment, SpringRate, Stress,
    Temperature,
};
