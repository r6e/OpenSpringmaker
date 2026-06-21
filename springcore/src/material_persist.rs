//! User-overlay persistence: load/save the editable materials file. The overlay
//! is untrusted input — a malformed file or bad entry yields warnings and a
//! curated-only fallback, never a panic.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SpringError};
use crate::material::{Material, MaterialSet, RawMaterial};
use crate::material_store::MaterialStore;

/// Current on-disk schema version for the user overlay.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A non-fatal problem encountered while loading the user overlay.
#[derive(Debug, Clone)]
pub struct LoadWarning {
    /// Human-readable description of the problem.
    pub message: String,
}

impl LoadWarning {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct OverlayDoc {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    material: Vec<RawMaterial>,
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

/// Resolve the user overlay file path in the OS config directory.
pub fn user_overlay_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("co", "r6e", "OpenSpringmaker")
        .map(|pd| pd.config_dir().join("materials.toml"))
}

/// Serialize user materials to the overlay TOML (with schema version).
pub fn serialize_user_materials(materials: &[Material]) -> Result<String> {
    let doc = OverlayDoc {
        schema_version: CURRENT_SCHEMA_VERSION,
        material: materials.iter().map(Material::to_raw).collect(),
    };
    toml::to_string_pretty(&doc).map_err(|e| SpringError::DataFile(e.to_string()))
}

/// Parse the overlay TOML into materials, collecting per-entry warnings.
/// A whole-file parse error yields no materials and a single warning.
pub fn parse_user_overlay(s: &str) -> (Vec<Material>, Vec<LoadWarning>) {
    let doc: OverlayDoc = match toml::from_str(s) {
        Ok(d) => d,
        Err(e) => {
            return (
                Vec::new(),
                vec![LoadWarning::new(format!(
                    "user materials file is malformed and was ignored: {e}"
                ))],
            )
        }
    };
    let mut materials = Vec::new();
    let mut warnings = Vec::new();
    if doc.schema_version > CURRENT_SCHEMA_VERSION {
        warnings.push(LoadWarning::new(format!(
            "user materials file schema_version {} is newer than supported {}; loading best-effort",
            doc.schema_version, CURRENT_SCHEMA_VERSION
        )));
    }
    for raw in doc.material {
        let name = raw.name.clone();
        match Material::try_from_raw(raw) {
            Ok(m) => materials.push(m),
            Err(e) => warnings.push(LoadWarning::new(format!(
                "skipping user material '{name}': {e}"
            ))),
        }
    }
    (materials, warnings)
}

fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "overlay path has no parent directory",
        )
    })?;
    let tmp = dir.join(format!(".materials.{}.tmp", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

impl MaterialStore {
    /// Build a store from a curated set and an overlay TOML string, applying the
    /// identity rules. Reserved-name / duplicate user entries are skipped with a
    /// warning (curated data is never overridden).
    pub fn from_overlay_str(curated: MaterialSet, s: &str) -> (Self, Vec<LoadWarning>) {
        let mut store = MaterialStore::new(curated);
        let (materials, mut warnings) = parse_user_overlay(s);
        for m in materials {
            let name = m.name.clone();
            if let Err(e) = store.add(m) {
                warnings.push(LoadWarning::new(format!(
                    "skipping user material '{name}': {e}"
                )));
            }
        }
        (store, warnings)
    }

    /// Build a store from a curated set and an overlay file path. A missing file
    /// is normal (curated-only, no warning); an unreadable file warns.
    pub fn from_overlay_file(curated: MaterialSet, path: &Path) -> (Self, Vec<LoadWarning>) {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_overlay_str(curated, &s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (MaterialStore::new(curated), Vec::new())
            }
            Err(e) => (
                MaterialStore::new(curated),
                vec![LoadWarning::new(format!(
                    "could not read user materials file: {e}"
                ))],
            ),
        }
    }

    /// Load curated + user overlay from the OS config dir.
    pub fn load() -> (Self, Vec<LoadWarning>) {
        let curated = MaterialSet::load_default();
        match user_overlay_path() {
            Some(p) => Self::from_overlay_file(curated, &p),
            None => (
                MaterialStore::new(curated),
                vec![LoadWarning::new(
                    "no OS config directory available; user materials not loaded",
                )],
            ),
        }
    }

    /// Write the user overlay to an explicit path (atomic).
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let toml = serialize_user_materials(self.user_materials())?;
        atomic_write(path, &toml).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    /// Write the user overlay to the OS config dir (creating it if needed).
    pub fn save(&self) -> Result<()> {
        let path = user_overlay_path()
            .ok_or_else(|| SpringError::DataFile("no OS config directory available".into()))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| SpringError::DataFile(e.to_string()))?;
        }
        self.save_to_path(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;
    use approx::assert_relative_eq;

    fn curated() -> MaterialSet {
        MaterialSet::load_default()
    }

    fn a_user_material(name: &str) -> crate::material::Material {
        let mut m = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        m.name = name.to_string();
        m
    }

    #[test]
    fn serialize_then_parse_roundtrips_user_material() {
        let m = a_user_material("My Wire");
        let toml = serialize_user_materials(std::slice::from_ref(&m)).unwrap();
        assert!(toml.contains("schema_version"));
        let (mats, warns) = parse_user_overlay(&toml);
        assert!(warns.is_empty());
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].name, "My Wire");
        // MTS value preserved (not merely parseable): compare the strength at a
        // reference diameter before vs after the round-trip. A corrupted
        // to_raw/try_from_raw conversion would change this value.
        let d = crate::units::Length::from_millimeters(1.0);
        let before = m.min_tensile_strength(d).unwrap().pascals();
        let after = mats[0].min_tensile_strength(d).unwrap().pascals();
        assert_relative_eq!(after, before, max_relative = 1e-9);
    }

    #[test]
    fn malformed_overlay_falls_back_to_curated_with_warning() {
        let (store, warns) =
            MaterialStore::from_overlay_str(curated(), "this is not valid toml {{{");
        assert!(!warns.is_empty());
        // curated still present, no user materials
        assert!(store.get("Music Wire").is_ok());
        assert!(store.user_materials().is_empty());
    }

    #[test]
    fn reserved_name_in_overlay_is_skipped_not_overriding_curated() {
        // A user file that tries to redefine a curated name.
        let m = a_user_material("Music Wire"); // reserved
        let toml = serialize_user_materials(std::slice::from_ref(&m)).unwrap();
        let (store, warns) = MaterialStore::from_overlay_str(curated(), &toml);
        assert!(warns.iter().any(|w| w.message.contains("Music Wire")));
        // The curated Music Wire is intact and there is no user override.
        assert!(store.is_curated("Music Wire"));
        assert!(store.user_materials().is_empty());
    }

    #[test]
    fn bad_entry_skipped_others_loaded() {
        // One good user material + one with an unknown form, hand-written.
        let good =
            serialize_user_materials(std::slice::from_ref(&a_user_material("Good Wire"))).unwrap();
        // Inject a bad [[material]] with an unknown form into the same doc.
        let bad = good.replace(
            "[[material]]",
            "[[material]]\nname = \"Bad\"\nspecification = \"x\"\ncitations = \"x\"\nmts_form = \"banana\"\nmts_units = \"si_mpa_mm\"\nmts_coefficients = [1.0]\nvalid_dia_min_mm = 1.0\nvalid_dia_max_mm = 10.0\nyoungs_modulus_gpa = 200.0\nshear_modulus_gpa = 78.0\ndensity_kg_per_m3 = 7850.0\nallowable_pct_torsion = 0.45\nallowable_pct_bending = 0.75\nallowable_pct_set = 0.6\n\n[[material]]",
        );
        let (mats, warns) = parse_user_overlay(&bad);
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].name, "Good Wire");
        assert!(warns.iter().any(|w| w.message.contains("Bad")));
    }

    #[test]
    fn non_finite_toml_literal_entry_is_skipped_others_loaded() {
        // The TOML format admits `nan`/`inf` float literals, so an untrusted
        // overlay can carry a non-finite numeric field. It must be skipped with
        // a warning (never panic, never let NaN/inf reach a calculation), while
        // valid entries in the same file still load.
        let good =
            serialize_user_materials(std::slice::from_ref(&a_user_material("Good Wire"))).unwrap();
        let bad = good.replace(
            "[[material]]",
            "[[material]]\nname = \"NaN Wire\"\nspecification = \"x\"\ncitations = \"x\"\nmts_form = \"power_law\"\nmts_units = \"si_mpa_mm\"\nmts_coefficients = [2000.0, 0.1]\nvalid_dia_min_mm = 1.0\nvalid_dia_max_mm = 10.0\nyoungs_modulus_gpa = nan\nshear_modulus_gpa = 78.0\ndensity_kg_per_m3 = 7850.0\nallowable_pct_torsion = 0.45\nallowable_pct_bending = 0.75\nallowable_pct_set = 0.6\n\n[[material]]",
        );
        let (mats, warns) = parse_user_overlay(&bad);
        assert_eq!(mats.len(), 1);
        assert_eq!(mats[0].name, "Good Wire");
        assert!(warns.iter().any(|w| w.message.contains("NaN Wire")));
        // And the loaded material is finite — no NaN/inf escaped into the store.
        assert!(mats[0].youngs_modulus.pascals().is_finite());
    }

    #[test]
    fn file_save_and_load_roundtrip() {
        let mut store = MaterialStore::new(curated());
        store.add(a_user_material("Disk Wire")).unwrap();
        let mut path = std::env::temp_dir();
        path.push(format!("osm_materials_test_{}.toml", std::process::id()));
        store.save_to_path(&path).unwrap();
        let (loaded, warns) = MaterialStore::from_overlay_file(curated(), &path);
        assert!(warns.is_empty());
        assert!(loaded.get("Disk Wire").is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_preserves_youngs_and_shear_modulus() {
        let orig = MaterialSet::load_default()
            .get("Music Wire")
            .unwrap()
            .clone();
        let orig_youngs = orig.youngs_modulus.pascals();
        let orig_shear = orig.shear_modulus.pascals();
        let toml = serialize_user_materials(std::slice::from_ref(&orig)).unwrap();
        let (mats, warns) = parse_user_overlay(&toml);
        assert!(warns.is_empty());
        assert_eq!(mats.len(), 1);
        assert_relative_eq!(
            mats[0].youngs_modulus.pascals(),
            orig_youngs,
            max_relative = 1e-6
        );
        assert_relative_eq!(
            mats[0].shear_modulus.pascals(),
            orig_shear,
            max_relative = 1e-6
        );
    }

    #[test]
    fn future_schema_version_yields_warning() {
        let (_, warns) = parse_user_overlay("schema_version = 999\n");
        assert!(!warns.is_empty());
        assert!(warns.iter().any(|w| w.message.contains("schema_version")));
    }

    #[test]
    fn current_schema_version_yields_no_schema_warning() {
        let (_, warns) = parse_user_overlay("schema_version = 1\n");
        assert!(warns.is_empty());
    }

    #[test]
    fn unreadable_path_yields_warning() {
        // A path containing an interior NUL byte is rejected before the OS call
        // with ErrorKind::InvalidInput on every platform (Unix CString and
        // Windows wide-string conversion both reject NUL) — a portable way to
        // force a non-NotFound read error, exercising the "could not read" warning
        // branch. (A directory path is not portable here: Unix yields IsADirectory
        // but Windows does not surface a warning-triggering error.)
        let (_, warns) =
            MaterialStore::from_overlay_file(curated(), Path::new("bad\0overlay.toml"));
        assert!(!warns.is_empty());
    }

    #[test]
    fn user_overlay_path_ends_with_materials_toml() {
        let p = user_overlay_path().expect("config dir");
        assert!(p.ends_with("materials.toml"));
    }

    #[test]
    fn save_to_path_without_parent_is_data_error_not_panic() {
        // An empty path has no parent directory; atomic_write must error rather
        // than silently falling back to the current directory for the temp file.
        let store = MaterialStore::new(curated());
        let err = store.save_to_path(Path::new("")).unwrap_err();
        // Assert the specific no-parent message so the test pins the explicit
        // parent-check branch rather than an incidental rename failure.
        match err {
            SpringError::DataFile(msg) => {
                assert!(
                    msg.contains("no parent directory"),
                    "expected no-parent message, got: {msg}"
                );
            }
            other => panic!("expected DataFile, got {other:?}"),
        }
    }

    #[test]
    fn missing_overlay_file_is_curated_only_no_warning() {
        let mut path = std::env::temp_dir();
        path.push(format!("osm_materials_absent_{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (store, warns) = MaterialStore::from_overlay_file(curated(), &path);
        assert!(warns.is_empty());
        assert!(store.user_materials().is_empty());
        assert!(store.get("Music Wire").is_ok());
    }
}
