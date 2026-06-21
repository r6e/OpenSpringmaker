//! Shared test helpers. Only compiled in test builds.

use crate::material::{Material, MaterialSet};

/// Look up a named material from the default bundled set.
pub(crate) fn material(name: &str) -> Material {
    MaterialSet::load_default().get(name).unwrap().clone()
}

/// Convenience shorthand for the Music Wire entry.
pub(crate) fn music_wire() -> Material {
    material("Music Wire")
}
