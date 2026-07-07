//! Spring family discriminant — single source of truth (like `UnitSystem`).
use serde::{Deserialize, Serialize};

/// Which spring family a design belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Family {
    #[default]
    Compression,
    Extension,
    Torsion,
    Conical,
}

impl std::fmt::Display for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Family::Compression => "Compression",
            Family::Extension => "Extension",
            Family::Torsion => "Torsion",
            Family::Conical => "Conical",
        })
    }
}

/// All families in selector display order.
pub const ALL_FAMILIES: &[Family] = &[
    Family::Compression,
    Family::Extension,
    Family::Torsion,
    Family::Conical,
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_compression() {
        assert_eq!(Family::default(), Family::Compression);
    }
    #[test]
    fn display_names_match_serde_tags() {
        assert_eq!(Family::Compression.to_string(), "Compression");
        assert_eq!(Family::Extension.to_string(), "Extension");
    }
    #[test]
    fn torsion_display_and_in_all_families() {
        assert_eq!(Family::Torsion.to_string(), "Torsion");
        assert!(ALL_FAMILIES.contains(&Family::Torsion));
    }
    #[test]
    fn conical_display_and_in_all_families() {
        assert_eq!(Family::Conical.to_string(), "Conical");
        assert!(ALL_FAMILIES.contains(&Family::Conical));
    }
}
