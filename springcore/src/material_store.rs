//! Mutable materials database: a read-only curated set plus an editable user
//! overlay. Curated names are reserved and curated materials are read-only;
//! user names are unique. Provenance is tracked by collection membership.

use crate::error::{Result, SpringError};
use crate::material::{Material, MaterialSet};

/// The merged, mutable material collection backing the editor and calculator.
#[derive(Debug, Clone)]
pub struct MaterialStore {
    curated: MaterialSet,
    user: Vec<Material>,
}

impl MaterialStore {
    /// Create a store from a curated set, with no user materials.
    pub fn new(curated: MaterialSet) -> Self {
        Self {
            curated,
            user: Vec::new(),
        }
    }

    /// All material names, curated first then user, in order.
    pub fn names(&self) -> Vec<&str> {
        let mut out = self.curated.names();
        out.extend(self.user.iter().map(|m| m.name.as_str()));
        out
    }

    /// Look up a material by name (curated first, then user).
    pub fn get(&self, name: &str) -> Result<&Material> {
        if let Ok(m) = self.curated.get(name) {
            return Ok(m);
        }
        self.user
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| SpringError::MaterialNotFound(name.to_string()))
    }

    /// True if `name` is a curated (read-only) material.
    pub fn is_curated(&self, name: &str) -> bool {
        self.curated.get(name).is_ok()
    }

    /// True if any material (curated or user) has this name.
    fn name_exists(&self, name: &str) -> bool {
        self.is_curated(name) || self.user.iter().any(|m| m.name == name)
    }

    /// Add a new user material. Rejects reserved (curated) names and duplicates.
    pub fn add(&mut self, material: Material) -> Result<()> {
        self.check_name_available(&material.name)?;
        self.user.push(material);
        Ok(())
    }

    fn check_name_available(&self, name: &str) -> Result<()> {
        if self.is_curated(name) {
            return Err(SpringError::InconsistentInputs(format!(
                "'{name}' is a reserved curated material name"
            )));
        }
        if self.user.iter().any(|m| m.name == name) {
            return Err(SpringError::InconsistentInputs(format!(
                "a user material named '{name}' already exists"
            )));
        }
        Ok(())
    }

    /// Replace the user material currently named `name` with `material`
    /// (whose name may differ — a rename). Curated materials are read-only.
    pub fn update(&mut self, name: &str, material: Material) -> Result<()> {
        if self.is_curated(name) {
            return Err(SpringError::InconsistentInputs(format!(
                "'{name}' is curated and read-only; clone it to make an editable copy"
            )));
        }
        let idx = self
            .user
            .iter()
            .position(|m| m.name == name)
            .ok_or_else(|| SpringError::MaterialNotFound(name.to_string()))?;
        // If renaming, the new name must be free. The old entry still holds the
        // old `name`, so this can't false-positive on the entry being edited.
        if material.name != name {
            self.check_name_available(&material.name)?;
        }
        self.user[idx] = material;
        Ok(())
    }

    /// Remove a user material. Curated materials cannot be removed.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if self.is_curated(name) {
            return Err(SpringError::InconsistentInputs(format!(
                "'{name}' is curated and cannot be removed"
            )));
        }
        let idx = self
            .user
            .iter()
            .position(|m| m.name == name)
            .ok_or_else(|| SpringError::MaterialNotFound(name.to_string()))?;
        self.user.remove(idx);
        Ok(())
    }

    /// Produce an editable copy of any material with a unique "(copy)" name.
    /// The result is NOT added to the store; the caller adds it after editing.
    pub fn clone_material(&self, name: &str) -> Result<Material> {
        let mut copy = self.get(name)?.clone();
        copy.name = self.unique_copy_name(name);
        Ok(copy)
    }

    fn unique_copy_name(&self, base: &str) -> String {
        let first = format!("{base} (copy)");
        if !self.name_exists(&first) {
            return first;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} (copy {n})");
            if !self.name_exists(&candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// The user overlay materials (for persistence).
    pub fn user_materials(&self) -> &[Material] {
        &self.user
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use crate::material::MaterialSet;

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    // Build a user material by cloning a curated one and renaming.
    fn user_material(
        store: &MaterialStore,
        from: &str,
        new_name: &str,
    ) -> crate::material::Material {
        let mut m = store.get(from).unwrap().clone();
        m.name = new_name.to_string();
        m
    }

    #[test]
    fn starts_with_curated_and_no_user() {
        let s = store();
        assert!(s.names().contains(&"Music Wire"));
        assert!(s.is_curated("Music Wire"));
        assert!(s.get("Music Wire").is_ok());
    }

    #[test]
    fn add_user_material() {
        let mut s = store();
        let m = user_material(&s, "Music Wire", "My Special Wire");
        s.add(m).unwrap();
        assert!(s.names().contains(&"My Special Wire"));
        assert!(!s.is_curated("My Special Wire"));
        assert!(s.get("My Special Wire").is_ok());
    }

    #[test]
    fn add_with_reserved_curated_name_is_rejected() {
        let mut s = store();
        let m = user_material(&s, "Music Wire", "Music Wire"); // reserved
        assert!(matches!(s.add(m), Err(SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn add_duplicate_user_name_is_rejected() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "Dup"))
            .unwrap();
        assert!(matches!(
            s.add(user_material(&s.clone(), "Music Wire", "Dup")),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn update_user_material() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "Editable"))
            .unwrap();
        let mut edited = s.get("Editable").unwrap().clone();
        edited.specification = "changed".into();
        s.update("Editable", edited).unwrap();
        assert_eq!(s.get("Editable").unwrap().specification, "changed");
    }

    #[test]
    fn update_curated_is_rejected_read_only() {
        let mut s = store();
        let m = s.get("Music Wire").unwrap().clone();
        assert!(matches!(
            s.update("Music Wire", m),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn update_missing_user_is_not_found() {
        let mut s = store();
        let m = user_material(&s.clone(), "Music Wire", "Ghost");
        assert!(matches!(
            s.update("Ghost", m),
            Err(SpringError::MaterialNotFound(_))
        ));
    }

    #[test]
    fn rename_user_material_works_but_not_onto_reserved_or_dup() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "A")).unwrap();
        s.add(user_material(&s.clone(), "Music Wire", "B")).unwrap();
        // valid rename A -> A2
        let mut a = s.get("A").unwrap().clone();
        a.name = "A2".into();
        s.update("A", a).unwrap();
        assert!(s.get("A").is_err() && s.get("A2").is_ok());
        // rename onto a curated name -> rejected
        let mut a2 = s.get("A2").unwrap().clone();
        a2.name = "Music Wire".into();
        assert!(matches!(
            s.update("A2", a2),
            Err(SpringError::InconsistentInputs(_))
        ));
        // rename onto an existing user name (B) -> rejected
        let mut a2b = s.get("A2").unwrap().clone();
        a2b.name = "B".into();
        assert!(matches!(
            s.update("A2", a2b),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn remove_user_only() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "Temp"))
            .unwrap();
        s.remove("Temp").unwrap();
        assert!(s.get("Temp").is_err());
        assert!(matches!(
            s.remove("Music Wire"),
            Err(SpringError::InconsistentInputs(_))
        ));
        assert!(matches!(
            s.remove("Nope"),
            Err(SpringError::MaterialNotFound(_))
        ));
    }

    #[test]
    fn clone_material_makes_unique_user_copy() {
        let mut s = store();
        let c1 = s.clone_material("Music Wire").unwrap();
        assert_eq!(c1.name, "Music Wire (copy)");
        s.add(c1).unwrap();
        let c2 = s.clone_material("Music Wire").unwrap();
        assert_eq!(c2.name, "Music Wire (copy 2)");
        assert!(matches!(
            s.clone_material("Nope"),
            Err(SpringError::MaterialNotFound(_))
        ));
    }

    #[test]
    fn clone_increments_to_copy_3() {
        let mut s = store();
        let c1 = s.clone_material("Music Wire").unwrap();
        s.add(c1).unwrap();
        let c2 = s.clone_material("Music Wire").unwrap();
        s.add(c2).unwrap();
        let c3 = s.clone_material("Music Wire").unwrap();
        assert_eq!(c3.name, "Music Wire (copy 3)");
    }
}
