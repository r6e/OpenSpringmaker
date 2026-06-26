//! App-level preferences persisted as `settings.toml` in the platform config
//! directory (same base as the materials overlay). v1 holds only the curvature-
//! correction preference; the struct is the home for future preferences.

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

    /// Persist to `path`, creating parent directories. Writes atomically (temp
    /// file + rename) so a crash mid-write can't corrupt the settings file.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let dir = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "settings path has no parent directory",
            )
        })?;
        std::fs::create_dir_all(dir)?;
        let toml = toml::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Write to a temp file then rename over the target (mirrors
        // springcore::material_persist::atomic_write). The temp name is unique per
        // process AND thread so concurrent saves — including cargo's test threads —
        // never share a temp path or race on the rename.
        let tmp = dir.join(format!(
            ".settings.{}.{:?}.tmp",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&tmp, toml)?;
        std::fs::rename(&tmp, path)
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
