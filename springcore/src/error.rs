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
    /// An error attributed to a specific member in a spring assembly.
    Member {
        /// Zero-based index of the member that failed.
        index: usize,
        /// The underlying error from that member's calculation.
        source: Box<SpringError>,
    },
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
            Self::Member { index, source } => {
                let inner = match source.as_ref() {
                    Self::InconsistentInputs(m) => m.clone(),
                    other => other.to_string(),
                };
                write!(f, "member {}: {inner}", index + 1)
            }
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

    #[test]
    fn member_display_is_byte_identical_to_the_old_flatten() {
        // InconsistentInputs source: the RAW inner message, no doubled prefix.
        let e = SpringError::Member {
            index: 1,
            source: Box::new(SpringError::InconsistentInputs(
                "mean diameter must be greater than wire diameter".into(),
            )),
        };
        assert_eq!(
            e.to_string(),
            "member 2: mean diameter must be greater than wire diameter"
        );
        // Non-InconsistentInputs source flattens via its own Display.
        let e = SpringError::Member {
            index: 0,
            source: Box::new(SpringError::MaterialNotFound("Unobtainium".into())),
        };
        assert_eq!(e.to_string(), "member 1: material not found: Unobtainium");
    }
}
