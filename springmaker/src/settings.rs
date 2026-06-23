//! App-level preferences persisted as `settings.toml` in the platform config
//! directory (same base as the materials overlay). v1 holds only the curvature-
//! correction preference; the struct is the home for future preferences.
// B2 wires these into App; until then suppress dead-code lint on the public API.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use springcore::CurvatureCorrection;
use std::path::{Path, PathBuf};

/// Persisted application preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Body-shear curvature-correction factor applied to all designs.
    pub curvature_correction: CurvatureCorrection,
}

/// Path to the settings file, or `None` if the platform config dir is unavailable.
pub fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("co", "r6e", "OpenSpringmaker")
        .map(|pd| pd.config_dir().join("settings.toml"))
}

/// Load settings from `path`; a missing or malformed file yields defaults.
pub fn load_from(path: &Path) -> AppSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

impl AppSettings {
    /// Load from the platform settings path (defaults if unavailable/malformed).
    pub fn load() -> Self {
        settings_path().map(|p| load_from(&p)).unwrap_or_default()
    }

    /// Persist to `path`, creating parent directories.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, toml)
    }

    /// Persist to the platform settings path; no-op if unavailable.
    pub fn save(&self) -> std::io::Result<()> {
        match settings_path() {
            Some(p) => self.save_to(&p),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use springcore::CurvatureCorrection;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("osm_settings_{}_{}.toml", name, std::process::id()))
    }

    #[test]
    fn round_trips() {
        let p = temp("round");
        AppSettings {
            curvature_correction: CurvatureCorrection::Wahl,
        }
        .save_to(&p)
        .unwrap();
        assert_eq!(
            load_from(&p).curvature_correction,
            CurvatureCorrection::Wahl
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_file_is_default() {
        let p = temp("missing");
        let _ = std::fs::remove_file(&p);
        assert_eq!(load_from(&p), AppSettings::default());
    }

    #[test]
    fn malformed_file_is_default() {
        let p = temp("malformed");
        std::fs::write(&p, "this is not = valid : toml ][").unwrap();
        assert_eq!(load_from(&p), AppSettings::default());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn default_is_bergstrasser() {
        assert_eq!(
            AppSettings::default().curvature_correction,
            CurvatureCorrection::Bergstrasser
        );
    }
}
