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

/// Load settings from `path`, returning the settings plus an optional warning.
///
/// A missing file (the normal first-run case) yields defaults silently
/// (`None`). A malformed file or a read error other than not-found also yields
/// defaults, but with `Some(warning)` so the caller can surface that the saved
/// preference was reset and why — rather than hiding the problem.
pub fn load_from(path: &Path) -> (AppSettings, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(settings) => (settings, None),
            Err(e) => (
                AppSettings::default(),
                Some(format!(
                    "ignored a malformed settings file; using defaults ({e})"
                )),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (AppSettings::default(), None),
        Err(e) => (
            AppSettings::default(),
            Some(format!("could not read settings; using defaults ({e})")),
        ),
    }
}

impl AppSettings {
    /// Load from the platform settings path. Returns defaults (with an optional
    /// warning) when the file is missing/malformed/unreadable, or when no config
    /// directory is available.
    pub fn load() -> (Self, Option<String>) {
        settings_path()
            .map(|p| load_from(&p))
            .unwrap_or((Self::default(), None))
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
        let (settings, warning) = load_from(&p);
        assert_eq!(settings.curvature_correction, CurvatureCorrection::Wahl);
        assert!(warning.is_none(), "a valid file yields no warning");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_file_is_default_without_warning() {
        let p = temp("missing");
        let _ = std::fs::remove_file(&p);
        // A missing file is the normal first-run case: defaults, no warning.
        assert_eq!(load_from(&p), (AppSettings::default(), None));
    }

    #[test]
    fn malformed_file_is_default_with_warning() {
        let p = temp("malformed");
        std::fs::write(&p, "this is not = valid : toml ][").unwrap();
        let (settings, warning) = load_from(&p);
        assert_eq!(settings, AppSettings::default());
        assert!(
            warning.is_some(),
            "a malformed file must surface a warning, not silently reset"
        );
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
