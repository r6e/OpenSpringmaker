//! Error type for all fallible spring calculations.

use std::fmt;

/// Errors returned by the spring calculation engine.
#[derive(Debug, Clone, PartialEq)]
pub enum SpringError {
    /// Inputs over-constrain or contradict the model.
    InconsistentInputs(String),
    /// A numeric solver hit its iteration cap without converging.
    NonConvergence { iterations: u32 },
    /// A root-find bracket did not contain a sign change.
    InvalidBracket,
    /// A constrained optimization found no feasible design.
    Infeasible(String),
    /// Wire diameter outside the material's valid range.
    DiameterOutOfRange {
        diameter_m: f64,
        min_m: f64,
        max_m: f64,
    },
    /// Fatigue requested for a material with no cited endurance data.
    NoFatigueData(String),
    /// Named material is not in the loaded set.
    MaterialNotFound(String),
    /// Material/persistence data file could not be read or parsed.
    DataFile(String),
}

impl fmt::Display for SpringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InconsistentInputs(m) => write!(f, "inconsistent inputs: {m}"),
            Self::NonConvergence { iterations } => {
                write!(
                    f,
                    "numeric solver did not converge after {iterations} iterations"
                )
            }
            Self::InvalidBracket => write!(f, "root-find bracket has no sign change"),
            Self::Infeasible(m) => write!(f, "no feasible design: {m}"),
            Self::DiameterOutOfRange {
                diameter_m,
                min_m,
                max_m,
            } => write!(
                f,
                "wire diameter {diameter_m} m outside valid range [{min_m}, {max_m}] m"
            ),
            Self::NoFatigueData(m) => write!(f, "no fatigue data available for {m}"),
            Self::MaterialNotFound(m) => write!(f, "material not found: {m}"),
            Self::DataFile(m) => write!(f, "data file error: {m}"),
        }
    }
}

impl std::error::Error for SpringError {}

/// Convenience result alias for the crate.
pub type Result<T> = std::result::Result<T, SpringError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        let e = SpringError::NonConvergence { iterations: 50 };
        assert_eq!(
            e.to_string(),
            "numeric solver did not converge after 50 iterations"
        );
        let e = SpringError::MaterialNotFound("A228".into());
        assert_eq!(e.to_string(), "material not found: A228");
    }

    #[test]
    fn is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&SpringError::InvalidBracket);
    }
}
