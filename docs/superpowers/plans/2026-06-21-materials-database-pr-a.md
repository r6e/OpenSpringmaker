# Materials Database PR (a) — Data Model + MaterialStore + Overlay Persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the immutable `MaterialSet` into the engine foundation for an editable materials database: add the rational MTS form, an informational temperature field, fallible (non-panicking) parsing, a mutable `MaterialStore` (curated read-only ∪ user overlay with the identity/merge rules), and config-dir overlay persistence — all in `springcore`, with the bundled curated set unchanged at 4 materials.

**Architecture:** `springcore::material` keeps the value types (`Material`, `MtsEquation`, `Endurance`, …), extended with `MtsForm::Rational` and `max_service_temperature`. A new `springcore::material_store` module holds `MaterialStore` (the merged collection + CRUD/identity rules). A new `springcore::material_persist` module handles the user overlay TOML (config-dir path, schema version, atomic write, graceful failure). The GUI is untouched in this PR (it keeps using `MaterialSet::load_default`); the editor and the GUI's switch to `MaterialStore` come in later PRs.

**Tech Stack:** Rust (edition 2021), serde + toml (existing), `directories` (new, config-dir resolution), approx (tests).

## Global Constraints

- **No commercial-product references** in any persisted file (code, comments, commits, data, docs).
- **Every formula/constant carries an inline citation.** (Data values land in PR (b); this PR adds no new material data.)
- **Strict TDD**: failing test → run red → minimal impl → run green → commit. Conventional commits; footer line on every commit:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- **Canonical internal units = SI**; material strength coefficients evaluated in native units (only the scalar result converted) — preserve this invariant.
- **No panics on untrusted input.** The user overlay file is untrusted: parse failures return `SpringError::DataFile` and the store falls back to curated-only with a surfaced warning — never a panic. The bundled curated file remains trusted (`load_default` may `expect`).
- **Identity/merge rule (binding):** name is the unique key; curated names are reserved (a user material may not reuse one); curated materials are read-only (clone to modify); rename = delete + add; merge = curated ∪ user with curated never overridden.
- Rust edition 2021; `cargo fmt` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean before each commit.

## Source abbreviations

- **Shigley** = Budynas & Nisbett, *Shigley's Mechanical Engineering Design*, 10th ed., Ch. 10.

---

## File Structure

```
springcore/
├── Cargo.toml                       # + directories dependency
├── data/materials.toml              # unchanged (still 4 materials)
└── src/
    ├── lib.rs                       # re-export new public types
    ├── units.rs                     # + Temperature newtype
    ├── material.rs                  # + Rational form, max_service_temperature, fallible parse, Material->Raw
    ├── material_store.rs            # NEW — MaterialStore (CRUD + identity/merge)
    └── material_persist.rs          # NEW — user-overlay TOML load/save (config dir, atomic, graceful)
```

`MaterialSet` stays as the curated loader (bundled, immutable). `MaterialStore` is the new mutable layer built on top; provenance is tracked by **collection membership** (curated vs user vectors) via `is_curated(name)` rather than a per-`Material` field — this is cleaner than the spec's tentative `source` field (no TOML change, a user file cannot falsely claim "curated") and achieves the same UI badging need.

---

## Task 1: Rational MTS form with denominator guard

**Files:**
- Modify: `springcore/src/material.rs`

**Interfaces:**
- Consumes: `MtsEquation`, `StrengthUnits`, `Length`, `Stress`, `Result`, `SpringError`.
- Produces: new variant `MtsForm::Rational`; `MtsEquation::evaluate` handles it; the `"rational"` string maps to it in parsing (parsing change lands in Task 3, but evaluate handles the variant now).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `springcore/src/material.rs`:

```rust
    // Rational MTS form: Sut = (P0*d^P4 + P1) / (P2*d^P4 + P3), d in mm, MPa.
    // Coeffs [1000, 500, 0.0, 2.0, 1.0] => Sut = (1000*d + 500) / (0 + 2) = 500*d + 250.
    // At d=3 mm -> 1750 MPa; at d=5 mm -> 2750 MPa (distinct).
    const RATIONAL_SAMPLE: &str = r#"
[[material]]
name = "Rational Test Wire"
specification = "synthetic"
citations = "synthetic test coefficients"
mts_form = "rational"
mts_units = "si_mpa_mm"
mts_coefficients = [1000.0, 500.0, 0.0, 2.0, 1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;

    #[test]
    fn rational_mts_evaluates_correctly() {
        let set = MaterialSet::from_toml_str(RATIONAL_SAMPLE).unwrap();
        let m = set.get("Rational Test Wire").unwrap();
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(3.0)).unwrap().megapascals(),
            500.0 * 3.0 + 250.0,
            max_relative = 1e-9
        );
        assert_relative_eq!(
            m.min_tensile_strength(Length::from_millimeters(5.0)).unwrap().megapascals(),
            500.0 * 5.0 + 250.0,
            max_relative = 1e-9
        );
    }

    #[test]
    fn rational_denominator_zero_is_rejected() {
        // [P0,P1,P2,P3,P4] = [1, 1, 1, -2, 1] => denom = d - 2; at d=2 -> 0.
        let toml = r#"
[[material]]
name = "Bad Rational"
specification = "synthetic"
citations = "synthetic"
mts_form = "rational"
mts_units = "si_mpa_mm"
mts_coefficients = [1.0, 1.0, 1.0, -2.0, 1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let set = MaterialSet::from_toml_str(toml).unwrap();
        let m = set.get("Bad Rational").unwrap();
        let err = m.min_tensile_strength(Length::from_millimeters(2.0)).unwrap_err();
        assert!(matches!(err, SpringError::InconsistentInputs(_)));
    }
```

(Note: these tests rely on `"rational"` parsing, added in Task 3. To keep Task 1 self-contained and red→green on its own, temporarily add the `"rational" => MtsForm::Rational,` arm to the existing `From<RawMaterial>` match in this task; Task 3 then moves that mapping into the fallible converter. If you prefer strict ordering, implement Task 3 first — the two are tightly coupled.)

- [ ] **Step 2: Run the tests, expect FAIL**

Run: `cargo test -p springcore material::tests::rational`
Expected: FAIL — `MtsForm::Rational` does not exist / `"rational"` unknown.

- [ ] **Step 3: Add the variant, the evaluate arm, and the parse arm**

In `springcore/src/material.rs`, extend `MtsForm`:

```rust
pub enum MtsForm {
    /// Sut = c0 (constant).
    Constant,
    /// Sut = A / d^m, coefficients = [A, m] (Shigley Eq. 10-14).
    PowerLaw,
    /// Sut = sum_i c_i d^i, coefficients = [c0, c1, ...].
    Polynomial,
    /// Sut = (P0*d^P4 + P1) / (P2*d^P4 + P3), coefficients = [P0, P1, P2, P3, P4].
    /// A 5-parameter rational curve fit. The denominator is guarded against zero.
    Rational,
}
```

In `MtsEquation::evaluate`, add the `Rational` arm to the `match self.form` (replacing the closing of the match):

```rust
        let raw = match self.form {
            MtsForm::Constant => c[0],
            MtsForm::PowerLaw => c[0] / dn.powf(c[1]),
            MtsForm::Polynomial => c
                .iter()
                .enumerate()
                .map(|(i, ci)| ci * dn.powi(i as i32))
                .sum::<f64>(),
            MtsForm::Rational => {
                let p = dn.powf(c[4]);
                let denominator = c[2] * p + c[3];
                let numerator = c[0] * p + c[1];
                let value = numerator / denominator;
                if !value.is_finite() {
                    return Err(SpringError::InconsistentInputs(format!(
                        "rational MTS denominator is zero or non-finite at diameter {} m",
                        d.meters()
                    )));
                }
                value
            }
        };
```

In the `From<RawMaterial>` match for `mts_form`, add: `"rational" => MtsForm::Rational,` (this moves to Task 3's fallible converter).

- [ ] **Step 4: Run the tests, expect PASS**

Run: `cargo test -p springcore material`
Expected: PASS (all existing + the two new rational tests).
Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean; `cargo fmt --all`.

- [ ] **Step 5: Commit**

```bash
git add springcore/src/material.rs
git commit -m "feat(material): add rational MTS form with denominator guard

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Temperature newtype and informational `max_service_temperature`

**Files:**
- Modify: `springcore/src/units.rs`, `springcore/src/material.rs`, `springcore/src/lib.rs`

**Interfaces:**
- Produces: `units::Temperature` with `from_celsius`/`from_fahrenheit`/`celsius()`/`fahrenheit()` (stored internally in °C); `Material.max_service_temperature: Option<Temperature>`; `RawMaterial` optional `max_service_temp_c`.

- [ ] **Step 1: Write the failing test (units)**

Add to `springcore/src/units.rs` test module:

```rust
    #[test]
    fn temperature_celsius_fahrenheit_roundtrip() {
        let t = Temperature::from_celsius(100.0);
        assert_relative_eq!(t.celsius(), 100.0, max_relative = 1e-12);
        assert_relative_eq!(t.fahrenheit(), 212.0, max_relative = 1e-12);
        let f = Temperature::from_fahrenheit(32.0);
        assert_relative_eq!(f.celsius(), 0.0, max_relative = 1e-12);
    }
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p springcore units::tests::temperature`
Expected: FAIL — `Temperature` not found.

- [ ] **Step 3: Add the `Temperature` newtype**

In `springcore/src/units.rs`, add another `si_quantity!` invocation and impl (stored in °C; this is an informational field, not used in calculations):

```rust
si_quantity!(
    /// Temperature, stored in degrees Celsius. Informational only — not used in
    /// any spring calculation.
    Temperature
);

impl Temperature {
    pub fn from_celsius(v: f64) -> Self { Self(v) }
    pub fn from_fahrenheit(v: f64) -> Self { Self((v - 32.0) * 5.0 / 9.0) }
    pub fn celsius(self) -> f64 { self.0 }
    pub fn fahrenheit(self) -> f64 { self.0 * 9.0 / 5.0 + 32.0 }
}
```

In `springcore/src/lib.rs`, add `Temperature` to the `pub use units::{...}` re-export list.

- [ ] **Step 4: Run units test, expect PASS**

Run: `cargo test -p springcore units::tests::temperature` → PASS.

- [ ] **Step 5: Write the failing test (material field)**

Add to `springcore/src/material.rs` tests:

```rust
    #[test]
    fn max_service_temperature_parses_when_present_and_absent() {
        // Present:
        let with_temp = r#"
[[material]]
name = "Temp Wire"
specification = "synthetic"
citations = "synthetic"
mts_form = "constant"
mts_units = "si_mpa_mm"
mts_coefficients = [1500.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
max_service_temp_c = 120.0
"#;
        let m = MaterialSet::from_toml_str(with_temp).unwrap();
        let mat = m.get("Temp Wire").unwrap();
        assert_relative_eq!(
            mat.max_service_temperature.unwrap().celsius(),
            120.0,
            max_relative = 1e-12
        );
        // Absent -> None (the existing SAMPLE has no max_service_temp_c).
        let s = MaterialSet::from_toml_str(SAMPLE).unwrap();
        assert!(s.get("Test Music Wire").unwrap().max_service_temperature.is_none());
    }
```

- [ ] **Step 6: Run, expect FAIL**

Run: `cargo test -p springcore material::tests::max_service`
Expected: FAIL — field `max_service_temperature` not found.

- [ ] **Step 7: Add the field and wire it**

In `springcore/src/material.rs`:
- add `use crate::units::Temperature;` to the units import (`use crate::units::{Length, MassDensity, Stress, Temperature};`).
- add to `Material`:

```rust
    /// Maximum service temperature, if specified. Informational only — NOT used
    /// in any calculation (no derating). Displayed with its citation.
    pub max_service_temperature: Option<Temperature>,
```

- add to `RawMaterial`:

```rust
    #[serde(default)]
    max_service_temp_c: Option<f64>,
```

- in the `From<RawMaterial>` construction, set:

```rust
            max_service_temperature: r.max_service_temp_c.map(Temperature::from_celsius),
```

- [ ] **Step 8: Run, expect PASS**

Run: `cargo test -p springcore material` → PASS.
Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean; `cargo fmt --all`.

- [ ] **Step 9: Commit**

```bash
git add springcore/src/units.rs springcore/src/material.rs springcore/src/lib.rs
git commit -m "feat(material): informational max_service_temperature + Temperature unit

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Fallible parsing (fix the deferred panic) + coefficient validation

**Files:**
- Modify: `springcore/src/material.rs`

**Interfaces:**
- Produces: `Material::try_from_raw(RawMaterial) -> Result<Material>` (replaces the panicking `From`); `mts_form_str(MtsForm) -> &'static str` and `strength_units_str(StrengthUnits) -> &'static str` helpers (used by persistence serialization in Task 5); coefficient-count validation per form.
- `MaterialSet::from_toml_str` now propagates parse errors as `SpringError::DataFile`. `load_default` keeps `expect` (bundled file is trusted).

Rationale: the user overlay is untrusted input. An unknown `mts_form`/`mts_units`, or a wrong number of coefficients (which would index-panic in `evaluate`, e.g. `c[1]` for a 1-element PowerLaw), must produce a typed error, not a panic.

- [ ] **Step 1: Write the failing tests**

Add to `springcore/src/material.rs` tests:

```rust
    #[test]
    fn unknown_mts_form_is_data_error_not_panic() {
        let toml = r#"
[[material]]
name = "Bad Form"
specification = "x"
citations = "x"
mts_form = "banana"
mts_units = "si_mpa_mm"
mts_coefficients = [1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let err = MaterialSet::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, SpringError::DataFile(_)));
    }

    #[test]
    fn unknown_mts_units_is_data_error() {
        let toml = r#"
[[material]]
name = "Bad Units"
specification = "x"
citations = "x"
mts_form = "constant"
mts_units = "furlongs"
mts_coefficients = [1.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        assert!(matches!(
            MaterialSet::from_toml_str(toml).unwrap_err(),
            SpringError::DataFile(_)
        ));
    }

    #[test]
    fn wrong_coefficient_count_is_data_error() {
        // power_law needs exactly 2 coefficients; give 1.
        let toml = r#"
[[material]]
name = "Bad Coeffs"
specification = "x"
citations = "x"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [2211.0]
valid_dia_min_mm = 1.0
valid_dia_max_mm = 10.0
youngs_modulus_gpa = 200.0
shear_modulus_gpa = 78.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        assert!(matches!(
            MaterialSet::from_toml_str(toml).unwrap_err(),
            SpringError::DataFile(_)
        ));
    }
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p springcore material::tests`
Expected: the three new tests FAIL — currently `from_toml_str` panics (test aborts) rather than returning `Err(DataFile)`.

- [ ] **Step 3: Replace the panicking `From` with fallible parsing**

In `springcore/src/material.rs`:

Add string<->enum helpers (used by parse and by Task 5 serialization) and a required-coefficient-count helper near `MtsForm`:

```rust
/// Stable string key for an MTS form (used in TOML).
pub(crate) fn mts_form_str(form: MtsForm) -> &'static str {
    match form {
        MtsForm::Constant => "constant",
        MtsForm::PowerLaw => "power_law",
        MtsForm::Polynomial => "polynomial",
        MtsForm::Rational => "rational",
    }
}

fn mts_form_from_str(s: &str) -> Result<MtsForm> {
    Ok(match s {
        "constant" => MtsForm::Constant,
        "power_law" => MtsForm::PowerLaw,
        "polynomial" => MtsForm::Polynomial,
        "rational" => MtsForm::Rational,
        other => return Err(SpringError::DataFile(format!("unknown mts_form: {other}"))),
    })
}

/// Stable string key for a strength unit system (used in TOML).
pub(crate) fn strength_units_str(units: StrengthUnits) -> &'static str {
    match units {
        StrengthUnits::UsKpsiInch => "us_kpsi_inch",
        StrengthUnits::SiMpaMm => "si_mpa_mm",
    }
}

fn strength_units_from_str(s: &str) -> Result<StrengthUnits> {
    Ok(match s {
        "us_kpsi_inch" => StrengthUnits::UsKpsiInch,
        "si_mpa_mm" => StrengthUnits::SiMpaMm,
        other => return Err(SpringError::DataFile(format!("unknown mts_units: {other}"))),
    })
}

/// Number of coefficients each form requires. Polynomial requires >= 1
/// (checked separately as a minimum).
fn coefficients_ok(form: MtsForm, n: usize) -> bool {
    match form {
        MtsForm::Constant => n == 1,
        MtsForm::PowerLaw => n == 2,
        MtsForm::Rational => n == 5,
        MtsForm::Polynomial => n >= 1,
    }
}
```

Replace `impl From<RawMaterial> for Material { ... }` with a fallible converter:

```rust
impl Material {
    fn try_from_raw(r: RawMaterial) -> Result<Self> {
        let form = mts_form_from_str(&r.mts_form)?;
        let units = strength_units_from_str(&r.mts_units)?;
        if !coefficients_ok(form, r.mts_coefficients.len()) {
            return Err(SpringError::DataFile(format!(
                "material '{}': {} coefficients for form {}",
                r.name,
                r.mts_coefficients.len(),
                r.mts_form
            )));
        }
        if r.valid_dia_min_mm > r.valid_dia_max_mm {
            return Err(SpringError::DataFile(format!(
                "material '{}': valid_dia_min_mm > valid_dia_max_mm",
                r.name
            )));
        }
        Ok(Material {
            name: r.name,
            specification: r.specification,
            mts: MtsEquation {
                form,
                units,
                coefficients: r.mts_coefficients,
                valid_dia_min: Length::from_millimeters(r.valid_dia_min_mm),
                valid_dia_max: Length::from_millimeters(r.valid_dia_max_mm),
            },
            youngs_modulus: Stress::from_pascals(r.youngs_modulus_gpa * 1.0e9),
            shear_modulus: Stress::from_pascals(r.shear_modulus_gpa * 1.0e9),
            density: MassDensity::from_kg_per_m3(r.density_kg_per_m3),
            allowable_pct_torsion: r.allowable_pct_torsion,
            allowable_pct_bending: r.allowable_pct_bending,
            allowable_pct_set: r.allowable_pct_set,
            endurance: r.endurance.map(|e| Endurance {
                ssa: Stress::from_megapascals(e.ssa_mpa),
                ssm: Stress::from_megapascals(e.ssm_mpa),
                peened: e.peened,
            }),
            max_service_temperature: r.max_service_temp_c.map(Temperature::from_celsius),
            citations: r.citations,
        })
    }
}
```

Update `MaterialSet::from_toml_str` to use it (and update its now-stale `# Panics` doc):

```rust
    /// Parse a TOML document containing `[[material]]` entries.
    ///
    /// Returns `SpringError::DataFile` on malformed input (unknown form/units,
    /// wrong coefficient count, inverted diameter range, or TOML syntax errors).
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let raw: RawDoc = toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))?;
        let materials = raw
            .material
            .into_iter()
            .map(Material::try_from_raw)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { materials })
    }
```

Remove the now-unused `"rational" => MtsForm::Rational,` line you may have added to the old `From` in Task 1 (the old `From` impl is gone). Keep `load_default` as-is (it `expect`s, which is correct for the trusted bundled file).

- [ ] **Step 4: Run, expect PASS**

Run: `cargo test -p springcore material` → PASS (all, including the three new error tests and the Task 1/2 tests).
Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean; `cargo fmt --all`.

- [ ] **Step 5: Commit**

```bash
git add springcore/src/material.rs
git commit -m "fix(material): fallible parsing (no panic on bad input) + coeff validation

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: MaterialStore — curated ∪ user with identity/merge rules

**Files:**
- Create: `springcore/src/material_store.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `MaterialSet`, `Material`, `Result`, `SpringError`.
- Produces: `MaterialStore` with `new(MaterialSet)`, `names()`, `get(&str)`, `is_curated(&str)`, `add(Material)`, `update(&str, Material)`, `remove(&str)`, `clone_material(&str)`, `user_materials()`. Provenance = collection membership (no per-material field).

Identity rules (binding): curated names reserved; curated read-only; user names unique; rename = delete+add (validated in `update`).

- [ ] **Step 1: Write the failing tests**

In `springcore/src/material_store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use crate::material::MaterialSet;

    fn store() -> MaterialStore {
        MaterialStore::new(MaterialSet::load_default())
    }

    // Build a user material by cloning a curated one and renaming.
    fn user_material(store: &MaterialStore, from: &str, new_name: &str) -> crate::material::Material {
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
        s.add(user_material(&s.clone(), "Music Wire", "Dup")).unwrap();
        assert!(matches!(
            s.add(user_material(&s.clone(), "Music Wire", "Dup")),
            Err(SpringError::InconsistentInputs(_))
        ));
    }

    #[test]
    fn update_user_material() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "Editable")).unwrap();
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
        assert!(matches!(s.update("A2", a2), Err(SpringError::InconsistentInputs(_))));
        // rename onto an existing user name (B) -> rejected
        let mut a2b = s.get("A2").unwrap().clone();
        a2b.name = "B".into();
        assert!(matches!(s.update("A2", a2b), Err(SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn remove_user_only() {
        let mut s = store();
        s.add(user_material(&s.clone(), "Music Wire", "Temp")).unwrap();
        s.remove("Temp").unwrap();
        assert!(s.get("Temp").is_err());
        assert!(matches!(s.remove("Music Wire"), Err(SpringError::InconsistentInputs(_))));
        assert!(matches!(s.remove("Nope"), Err(SpringError::MaterialNotFound(_))));
    }

    #[test]
    fn clone_material_makes_unique_user_copy() {
        let mut s = store();
        let c1 = s.clone_material("Music Wire").unwrap();
        assert_eq!(c1.name, "Music Wire (copy)");
        s.add(c1).unwrap();
        let c2 = s.clone_material("Music Wire").unwrap();
        assert_eq!(c2.name, "Music Wire (copy 2)");
        assert!(matches!(s.clone_material("Nope"), Err(SpringError::MaterialNotFound(_))));
    }
}
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p springcore material_store`
Expected: FAIL — `MaterialStore` not found. (Add `pub mod material_store;` to lib.rs first so the test module compiles.)

- [ ] **Step 3: Implement `MaterialStore`**

`springcore/src/material_store.rs` (above the test module):

```rust
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
        Self { curated, user: Vec::new() }
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
        // If renaming, the new name must be free (ignoring the entry being edited).
        if material.name != name {
            if self.is_curated(&material.name) {
                return Err(SpringError::InconsistentInputs(format!(
                    "'{}' is a reserved curated material name",
                    material.name
                )));
            }
            if self.user.iter().any(|m| m.name == material.name) {
                return Err(SpringError::InconsistentInputs(format!(
                    "a user material named '{}' already exists",
                    material.name
                )));
            }
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
```

In `springcore/src/lib.rs` add:

```rust
pub mod material_store;
pub use material_store::MaterialStore;
```

- [ ] **Step 4: Run, expect PASS**

Run: `cargo test -p springcore material_store` → PASS (all tests).
Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean; `cargo fmt --all`.

- [ ] **Step 5: Commit**

```bash
git add springcore/src/material_store.rs springcore/src/lib.rs
git commit -m "feat(material): MaterialStore with curated/user identity and merge rules

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: User-overlay persistence (config dir, schema version, atomic, graceful)

**Files:**
- Modify: `springcore/Cargo.toml` (add `directories`), `springcore/src/material.rs` (Raw serialize + `to_raw`, make raw types/`try_from_raw` `pub(crate)`), `springcore/src/lib.rs`
- Create: `springcore/src/material_persist.rs`

**Interfaces:**
- Consumes: `MaterialStore`, `Material`, raw types + `Material::try_from_raw`/`to_raw`, `mts_form_str`/`strength_units_str`.
- Produces: `LoadWarning`; `serialize_user_materials(&[Material]) -> Result<String>`; `parse_user_overlay(&str) -> (Vec<Material>, Vec<LoadWarning>)`; `user_overlay_path() -> Option<PathBuf>`; and on `MaterialStore`: `from_overlay_str`, `from_overlay_file`, `save_to_path`, `load`, `save`.

- [ ] **Step 1: Add the dependency**

In `springcore/Cargo.toml` `[dependencies]`: `directories = "5"` (add `directories = "5"` to the workspace `[workspace.dependencies]` too, and reference as `directories = { workspace = true }` if the workspace uses that pattern; otherwise a direct version is fine).

- [ ] **Step 2: Make raw types serializable and add `to_raw` (material.rs)**

In `springcore/src/material.rs`:
- change `use serde::Deserialize;` to `use serde::{Deserialize, Serialize};`
- make the raw types `pub(crate)` and add `Serialize`, with skip-if-none on options:

```rust
#[derive(Deserialize, Serialize)]
pub(crate) struct RawDoc {
    pub(crate) material: Vec<RawMaterial>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RawMaterial {
    pub(crate) name: String,
    pub(crate) specification: String,
    pub(crate) citations: String,
    pub(crate) mts_form: String,
    pub(crate) mts_units: String,
    pub(crate) mts_coefficients: Vec<f64>,
    pub(crate) valid_dia_min_mm: f64,
    pub(crate) valid_dia_max_mm: f64,
    pub(crate) youngs_modulus_gpa: f64,
    pub(crate) shear_modulus_gpa: f64,
    pub(crate) density_kg_per_m3: f64,
    pub(crate) allowable_pct_torsion: f64,
    pub(crate) allowable_pct_bending: f64,
    pub(crate) allowable_pct_set: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endurance: Option<RawEndurance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_service_temp_c: Option<f64>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RawEndurance {
    pub(crate) ssa_mpa: f64,
    pub(crate) ssm_mpa: f64,
    pub(crate) peened: bool,
}
```

- make `try_from_raw` `pub(crate)`: `pub(crate) fn try_from_raw(r: RawMaterial) -> Result<Self>`.
- add a `to_raw` method (inverse of `try_from_raw`) on `Material`:

```rust
    /// Convert back to the serializable raw form (for the user overlay file).
    pub(crate) fn to_raw(&self) -> RawMaterial {
        RawMaterial {
            name: self.name.clone(),
            specification: self.specification.clone(),
            citations: self.citations.clone(),
            mts_form: mts_form_str(self.mts.form).to_string(),
            mts_units: strength_units_str(self.mts.units).to_string(),
            mts_coefficients: self.mts.coefficients.clone(),
            valid_dia_min_mm: self.mts.valid_dia_min.millimeters(),
            valid_dia_max_mm: self.mts.valid_dia_max.millimeters(),
            youngs_modulus_gpa: self.youngs_modulus.pascals() / 1.0e9,
            shear_modulus_gpa: self.shear_modulus.pascals() / 1.0e9,
            density_kg_per_m3: self.density.kg_per_m3(),
            allowable_pct_torsion: self.allowable_pct_torsion,
            allowable_pct_bending: self.allowable_pct_bending,
            allowable_pct_set: self.allowable_pct_set,
            endurance: self.endurance.map(|e| RawEndurance {
                ssa_mpa: e.ssa.megapascals(),
                ssm_mpa: e.ssm.megapascals(),
                peened: e.peened,
            }),
            max_service_temp_c: self.max_service_temperature.map(|t| t.celsius()),
        }
    }
```

(Keep `RawDoc` parse path in `from_toml_str` working — the bundled file has no `schema_version`, and `RawDoc` has no such field, so the overlay's versioned doc is a separate type in `material_persist`.)

- [ ] **Step 3: Write the failing tests (persistence)**

In `springcore/src/material_persist.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;

    fn curated() -> MaterialSet {
        MaterialSet::load_default()
    }

    fn a_user_material(name: &str) -> crate::material::Material {
        let mut m = MaterialSet::load_default().get("Music Wire").unwrap().clone();
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
        // MTS preserved.
        assert!(mats[0]
            .min_tensile_strength(crate::units::Length::from_millimeters(1.0))
            .is_ok());
    }

    #[test]
    fn malformed_overlay_falls_back_to_curated_with_warning() {
        let (store, warns) = MaterialStore::from_overlay_str(curated(), "this is not valid toml {{{");
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
        let good = serialize_user_materials(std::slice::from_ref(&a_user_material("Good Wire"))).unwrap();
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
```

- [ ] **Step 4: Run, expect FAIL**

Run: `cargo test -p springcore material_persist`
Expected: FAIL — module/functions not found. (Add `pub mod material_persist;` to lib.rs first.)

- [ ] **Step 5: Implement `material_persist.rs`**

```rust
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
    pub message: String,
}

impl LoadWarning {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
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
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
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
                vec![LoadWarning::new("no OS config directory available; user materials not loaded")],
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
```

In `springcore/src/lib.rs` add:

```rust
pub mod material_persist;
pub use material_persist::{user_overlay_path, LoadWarning};
```

- [ ] **Step 6: Run, expect PASS**

Run: `cargo test -p springcore material_persist` → PASS.
Run: `cargo test --workspace` → all pass.
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean; `cargo fmt --all`.

- [ ] **Step 7: Commit**

```bash
git add springcore/Cargo.toml springcore/src/material.rs springcore/src/material_persist.rs springcore/src/lib.rs
git commit -m "feat(material): user-overlay persistence (config dir, atomic, graceful)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final verification

- [ ] **Step 1: Full gate**

Run: `cargo fmt --all -- --check` → clean.
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean.
Run: `cargo test --workspace` → all pass (springmaker untouched; bundled set still 4 materials).
Run: `cargo build --workspace` → builds.

- [ ] **Step 2: Mutation gate on the changed springcore logic**

Run: `cargo mutants --file springcore/src/material.rs --file springcore/src/material_store.rs --file springcore/src/material_persist.rs` → 0 missed (add tests for any survivor; persistence/parse logic is in scope, data values are not — there are none new here).

- [ ] **Step 3: Pre-push adversarial panel**

Run the mandatory multi-reviewer panel (general/security, architect, simplifier; add a numerical-methods reviewer for the rational-form guard) on the branch diff, cycle to convergence, then open the PR. Do not push without convergence.

