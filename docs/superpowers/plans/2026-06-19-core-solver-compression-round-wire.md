# Core Solver + Compression Cylindrical Round-Wire Calculator — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust workspace whose `springcore` library computes cylindrical
compression round-wire spring designs (rate, stresses, geometry, natural frequency,
buckling, fatigue) under four determined solve scenarios plus a minimum-weight
optimization, with a `springmaker` iced GUI calculator that solves live, plots
results, and saves/loads designs.

**Architecture:** Two-crate Cargo workspace. `springcore` is pure Rust (no GUI deps):
newtype unit quantities over canonical SI, a TOML-driven curated materials model with
unit-native strength coefficients, cited governing formulas, a `Scenario` trait with
one solver per mode, a numeric root-find kernel, a min-weight optimizer, and a
per-material modified-Goodman fatigue model. `springmaker` is an iced application
depending only on `springcore`'s public API.

**Tech Stack:** Rust (edition 2021), iced 0.13 (GUI), plotters 0.3 + plotters-iced
0.11 (charts), serde 1 + toml 0.8 (data files / persistence), approx 0.5 + proptest 1
(tests).

## Global Constraints

- **No commercial-product references** in any persisted file (code, comments, commit
  messages, data files, docs): never name the inspiration product or its vendor.
  Describe functionality on its own terms; cite engineering literature only.
- **Every formula and constant carries an inline citation** (source + equation/section
  number) at its definition site.
- **Strict TDD**: write the failing test, run it, watch it fail, implement minimally,
  run it green, commit. One logical change per commit; conventional-commit messages.
- **Canonical internal units = SI** (metre, newton, pascal, N/m, hertz, kg, radian).
  Convert only at the UI/data boundary. **Material strength coefficients are evaluated
  in their native units; only the scalar result is converted — never the coefficients.**
- **Rust edition 2021**; `cargo fmt` clean and `cargo clippy -- -D warnings` clean
  before every commit.
- **License:** dual MIT / Apache-2.0. Every source file is covered by it (no per-file
  headers required; LICENSE files at root).
- Commit message footer line for every commit:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

## Source abbreviations used in citations

- **Shigley** = Budynas & Nisbett, *Shigley's Mechanical Engineering Design*, 10th ed.,
  McGraw-Hill, 2015, Ch. 10.
- **Wahl** = A. M. Wahl, *Mechanical Springs*, 2nd ed., McGraw-Hill, 1963.
- **EN 13906-1** = EN 13906-1:2013, *Cylindrical helical springs made from round wire
  and bar — Calculation and design — Part 1: Compression springs*.
- **SMI** = Spring Manufacturers Institute, *Handbook of Spring Design*.
- **ASTM** = the cited ASTM material specification (A228, A229, A313, A401).
- **Zimmerli** = F. P. Zimmerli, "Human Failures in Spring Applications," 1957
  (endurance limits for steel spring wire), as tabulated in Shigley §10-9.

---

## File Structure

```
OpenSpringmaker/
├── Cargo.toml                      # workspace manifest
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── ARCHITECTURE.md
├── CONTRIBUTING.md
├── rustfmt.toml
├── clippy.toml
├── .gitignore
├── .github/workflows/ci.yml
├── docs/adr/
│   ├── 0001-scenario-based-solver.md
│   ├── 0002-si-internal-newtype-units.md
│   ├── 0003-unit-native-mts-coefficients.md
│   ├── 0004-two-crate-workspace.md
│   └── 0005-absolute-stability-buckling.md
├── springcore/
│   ├── Cargo.toml
│   ├── data/materials.toml         # curated, versioned material set
│   ├── src/
│   │   ├── lib.rs                  # public API re-exports
│   │   ├── units.rs                # newtype SI quantities
│   │   ├── unit_system.rs          # US/metric display conversion
│   │   ├── error.rs                # SpringError
│   │   ├── material.rs             # Material, MtsEquation, loader
│   │   ├── end_type.rs             # EndType + coil/length relations
│   │   ├── geometry.rs             # index, coils, solid/free length
│   │   ├── mechanics.rs            # rate, Wahl, stress, frequency, buckling
│   │   ├── numeric.rs              # root-find kernel
│   │   ├── fatigue.rs              # modified-Goodman, endurance data
│   │   ├── design.rs               # SpringDesign aggregate + status warnings
│   │   ├── scenario.rs             # Scenario trait + 4 determined scenarios
│   │   ├── optimize.rs             # min-weight constrained optimization
│   │   └── persistence.rs          # TOML save/load of a design
│   └── tests/
│       └── golden.rs               # published worked-example cross-checks
└── springmaker/
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # iced app entry
        ├── form.rs                 # pure, tested form-to-design logic
        ├── app.rs                  # state, Message, update
        ├── view.rs                 # form layout
        └── plot.rs                 # plotters-iced results chart
```

---

## Task 1: Workspace scaffolding

**Files:**
- Create: `Cargo.toml` (workspace), `springcore/Cargo.toml`, `springcore/src/lib.rs`,
  `springmaker/Cargo.toml`, `springmaker/src/main.rs`
- Create: `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`, `ARCHITECTURE.md`,
  `CONTRIBUTING.md`, `rustfmt.toml`, `clippy.toml`, `.gitignore`,
  `.github/workflows/ci.yml`
- Create: `docs/adr/0001-scenario-based-solver.md`,
  `docs/adr/0002-si-internal-newtype-units.md`,
  `docs/adr/0003-unit-native-mts-coefficients.md`,
  `docs/adr/0004-two-crate-workspace.md`,
  `docs/adr/0005-absolute-stability-buckling.md`

**Interfaces:**
- Produces: a compiling two-crate workspace. `springcore` exposes an empty public API
  (`lib.rs`); `springmaker` has a stub `main`. Later tasks add modules to `springcore`.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["springcore", "springmaker"]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/REPLACE_WITH_ACTUAL_REPO/OpenSpringmaker"
rust-version = "1.80"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
toml = "0.8"
approx = "0.5"
proptest = "1"
```

(Set `repository` to the real URL once the repo exists; it must not name any
commercial product.)

- [ ] **Step 2: Create `springcore/Cargo.toml`**

```toml
[package]
name = "springcore"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Engineering calculations for helical compression springs"

[dependencies]
serde = { workspace = true }
toml = { workspace = true }

[dev-dependencies]
approx = { workspace = true }
proptest = { workspace = true }
```

- [ ] **Step 3: Create `springcore/src/lib.rs`**

```rust
//! Engineering calculations for helical compression springs.
//!
//! All public quantities are stored internally in SI units. See the crate
//! `ARCHITECTURE.md` and `docs/adr/` for design rationale.

// Modules are added by later tasks.
```

- [ ] **Step 4: Create `springmaker/Cargo.toml`**

```toml
[package]
name = "springmaker"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Desktop calculator for helical compression spring design"

[dependencies]
springcore = { path = "../springcore" }
iced = "0.13"
plotters = "0.3"
plotters-iced = "0.11"
```

- [ ] **Step 5: Create `springmaker/src/main.rs` stub**

```rust
fn main() {
    println!("springmaker GUI is implemented in a later task.");
}
```

- [ ] **Step 6: Create config + license + doc files**

`rustfmt.toml`:

```toml
edition = "2021"
max_width = 100
```

`clippy.toml`:

```toml
# Curated thresholds; rules themselves are enabled via `-D warnings` in CI.
too-many-arguments-threshold = 10
```

`.gitignore`:

```
/target
**/*.rs.bk
Cargo.lock.orig
```

`LICENSE-MIT` and `LICENSE-APACHE`: paste the standard MIT and Apache-2.0 license
texts (year 2026, copyright holder "OpenSpringmaker contributors"). Do not invent
text; use the canonical SPDX texts.

`README.md`: project name, one-paragraph description (use the approved GitHub
description), build/run instructions (`cargo run -p springmaker`), test instructions
(`cargo test`), and the dual-license note. No commercial-product references.

`ARCHITECTURE.md`: summarize the two-crate split, SI-internal newtype units, the
scenario solver, and the materials data model. Link to `docs/adr/`.

`CONTRIBUTING.md`: TDD workflow, `cargo fmt` + `cargo clippy -- -D warnings` gate,
conventional commits, citation requirement for any new formula/constant.

- [ ] **Step 7: Create the five ADRs**

Each ADR uses the format: Title, Status (Accepted), Context, Decision, Consequences.
Write them from the approved spec:
- `0001-scenario-based-solver.md` — four determined closed-form scenarios + numeric
  kernel + min-weight optimizer; chosen over a generic constraint solver for
  determinism and testability.
- `0002-si-internal-newtype-units.md` — canonical SI internals via newtype quantities;
  convert at boundaries; rationale vs `uom`.
- `0003-unit-native-mts-coefficients.md` — strength coefficients evaluated in native
  units, only scalar result converted; cites Shigley Table 10-4 unit-dependence.
- `0004-two-crate-workspace.md` — engine has no GUI deps; explicit public API.
- `0005-absolute-stability-buckling.md` — v1 implements the absolute-stability
  criterion (Shigley Eq. 10-10); the deflection-ratio curve (Eq. 10-11) is deferred to
  avoid edition-dependent constants. Conservative and cited.

- [ ] **Step 8: Create CI workflow**

`.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace
      - name: Build
        run: cargo build --workspace
```

- [ ] **Step 9: Verify it builds**

Run: `cargo build --workspace`
Expected: compiles cleanly (both crates), no warnings.

Run: `cargo fmt --all -- --check` → no diff.
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "chore: scaffold two-crate workspace with CI, licenses, ADRs

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Newtype unit quantities

**Files:**
- Create: `springcore/src/units.rs`
- Modify: `springcore/src/lib.rs` (add `pub mod units;` and re-exports)

**Interfaces:**
- Produces: newtypes each wrapping `f64` in SI, with `from_*`/`to_*` constructors and
  accessors. Signatures relied on by every later task:
  - `Length::from_meters(f64) -> Length`, `Length::from_millimeters(f64) -> Length`,
    `Length::from_inches(f64) -> Length`; `.meters()`, `.millimeters()`, `.inches()`.
  - `Force::from_newtons(f64)`, `Force::from_pounds_force(f64)`; `.newtons()`,
    `.pounds_force()`.
  - `Stress::from_pascals(f64)`, `Stress::from_psi(f64)`, `Stress::from_megapascals(f64)`;
    `.pascals()`, `.psi()`, `.megapascals()`.
  - `SpringRate::from_newtons_per_meter(f64)`, `::from_pounds_per_inch(f64)`;
    `.newtons_per_meter()`, `.pounds_per_inch()`.
  - `Frequency::from_hertz(f64)`; `.hertz()`.
  - `MassDensity::from_kg_per_m3(f64)`, `::from_pounds_per_in3(f64)`; `.kg_per_m3()`.
  - `Modulus` = alias usage of `Stress` (E and G are stresses). Use `Stress` for moduli.
  - All types derive `Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize`.

- [ ] **Step 1: Write the failing test**

`springcore/src/units.rs` (test module at the bottom — write this first):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    // Conversion factors per NIST SP 811 (exact): 1 in = 0.0254 m,
    // 1 lbf = 4.4482216152605 N, 1 psi = 6894.757293168 Pa.
    #[test]
    fn length_inch_roundtrip() {
        let l = Length::from_inches(1.0);
        assert_relative_eq!(l.meters(), 0.0254, max_relative = 1e-12);
        assert_relative_eq!(l.inches(), 1.0, max_relative = 1e-12);
        assert_relative_eq!(l.millimeters(), 25.4, max_relative = 1e-12);
    }

    #[test]
    fn force_pound_roundtrip() {
        let f = Force::from_pounds_force(1.0);
        assert_relative_eq!(f.newtons(), 4.4482216152605, max_relative = 1e-12);
        assert_relative_eq!(f.pounds_force(), 1.0, max_relative = 1e-12);
    }

    #[test]
    fn stress_psi_roundtrip() {
        let s = Stress::from_psi(1.0);
        assert_relative_eq!(s.pascals(), 6894.757293168, max_relative = 1e-12);
        assert_relative_eq!(s.psi(), 1.0, max_relative = 1e-12);
        assert_relative_eq!(Stress::from_megapascals(1.0).pascals(), 1.0e6, max_relative = 1e-12);
    }

    #[test]
    fn rate_pounds_per_inch_roundtrip() {
        // 1 lbf/in = 4.4482216152605 / 0.0254 N/m = 175.126835... N/m
        let k = SpringRate::from_pounds_per_inch(1.0);
        assert_relative_eq!(k.newtons_per_meter(), 4.4482216152605 / 0.0254, max_relative = 1e-12);
        assert_relative_eq!(k.pounds_per_inch(), 1.0, max_relative = 1e-12);
    }

    #[test]
    fn density_pound_per_in3_roundtrip() {
        // 1 lb/in^3 = 27679.9047 kg/m^3 (derived from lbm and inch definitions)
        let d = MassDensity::from_pounds_per_in3(1.0);
        assert_relative_eq!(d.kg_per_m3(), 27679.904710203, max_relative = 1e-9);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore units`
Expected: FAIL — `Length` etc. not found (compile error).

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/units.rs` (above the test module):

```rust
//! Strongly-typed physical quantities. Each wraps an `f64` stored in SI base
//! units. Conversion factors are exact per NIST Special Publication 811.

use serde::{Deserialize, Serialize};

/// Exact unit-conversion constants (NIST SP 811).
const METERS_PER_INCH: f64 = 0.0254;
const NEWTONS_PER_LBF: f64 = 4.4482216152605;
const PASCALS_PER_PSI: f64 = 6894.757293168;
// 1 lb/in^3 = NEWTONS_PER_LBF/g_n converted... derived as mass: 1 lbm = 0.45359237 kg,
// 1 in^3 = 0.0254^3 m^3 -> 0.45359237 / 0.0254^3.
const KG_PER_M3_PER_LB_PER_IN3: f64 = 0.45359237 / (0.0254 * 0.0254 * 0.0254);

macro_rules! si_quantity {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(f64);
    };
}

si_quantity!(
    /// Length, stored in metres.
    Length
);
si_quantity!(
    /// Force, stored in newtons.
    Force
);
si_quantity!(
    /// Stress / pressure / elastic modulus, stored in pascals.
    Stress
);
si_quantity!(
    /// Spring rate (force per length), stored in newtons per metre.
    SpringRate
);
si_quantity!(
    /// Frequency, stored in hertz.
    Frequency
);
si_quantity!(
    /// Mass density, stored in kilograms per cubic metre.
    MassDensity
);

impl Length {
    pub fn from_meters(v: f64) -> Self { Self(v) }
    pub fn from_millimeters(v: f64) -> Self { Self(v / 1000.0) }
    pub fn from_inches(v: f64) -> Self { Self(v * METERS_PER_INCH) }
    pub fn meters(self) -> f64 { self.0 }
    pub fn millimeters(self) -> f64 { self.0 * 1000.0 }
    pub fn inches(self) -> f64 { self.0 / METERS_PER_INCH }
}

impl Force {
    pub fn from_newtons(v: f64) -> Self { Self(v) }
    pub fn from_pounds_force(v: f64) -> Self { Self(v * NEWTONS_PER_LBF) }
    pub fn newtons(self) -> f64 { self.0 }
    pub fn pounds_force(self) -> f64 { self.0 / NEWTONS_PER_LBF }
}

impl Stress {
    pub fn from_pascals(v: f64) -> Self { Self(v) }
    pub fn from_megapascals(v: f64) -> Self { Self(v * 1.0e6) }
    pub fn from_psi(v: f64) -> Self { Self(v * PASCALS_PER_PSI) }
    pub fn pascals(self) -> f64 { self.0 }
    pub fn megapascals(self) -> f64 { self.0 / 1.0e6 }
    pub fn psi(self) -> f64 { self.0 / PASCALS_PER_PSI }
}

impl SpringRate {
    pub fn from_newtons_per_meter(v: f64) -> Self { Self(v) }
    pub fn from_pounds_per_inch(v: f64) -> Self { Self(v * NEWTONS_PER_LBF / METERS_PER_INCH) }
    pub fn newtons_per_meter(self) -> f64 { self.0 }
    pub fn pounds_per_inch(self) -> f64 { self.0 * METERS_PER_INCH / NEWTONS_PER_LBF }
}

impl Frequency {
    pub fn from_hertz(v: f64) -> Self { Self(v) }
    pub fn hertz(self) -> f64 { self.0 }
}

impl MassDensity {
    pub fn from_kg_per_m3(v: f64) -> Self { Self(v) }
    pub fn from_pounds_per_in3(v: f64) -> Self { Self(v * KG_PER_M3_PER_LB_PER_IN3) }
    pub fn kg_per_m3(self) -> f64 { self.0 }
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod units;
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore units`
Expected: PASS (5 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/units.rs springcore/src/lib.rs
git commit -m "feat(units): SI newtype quantities with exact NIST conversions

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Error type

**Files:**
- Create: `springcore/src/error.rs`
- Modify: `springcore/src/lib.rs` (add `pub mod error;` and re-exports)

**Interfaces:**
- Produces: `SpringError` enum and `pub type Result<T> = std::result::Result<T, SpringError>;`
  used as the return type of every fallible function in later tasks. Variants:
  `InconsistentInputs(String)`, `NonConvergence { iterations: u32 }`, `InvalidBracket`,
  `Infeasible(String)`, `DiameterOutOfRange { diameter_m: f64, min_m: f64, max_m: f64 }`,
  `NoFatigueData(String)`, `MaterialNotFound(String)`, `DataFile(String)`.

- [ ] **Step 1: Write the failing test**

In `springcore/src/error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        let e = SpringError::NonConvergence { iterations: 50 };
        assert_eq!(e.to_string(), "numeric solver did not converge after 50 iterations");
        let e = SpringError::MaterialNotFound("A228".into());
        assert_eq!(e.to_string(), "material not found: A228");
    }

    #[test]
    fn is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&SpringError::InvalidBracket);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore error`
Expected: FAIL — `SpringError` not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/error.rs`:

```rust
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
    DiameterOutOfRange { diameter_m: f64, min_m: f64, max_m: f64 },
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
                write!(f, "numeric solver did not converge after {iterations} iterations")
            }
            Self::InvalidBracket => write!(f, "root-find bracket has no sign change"),
            Self::Infeasible(m) => write!(f, "no feasible design: {m}"),
            Self::DiameterOutOfRange { diameter_m, min_m, max_m } => write!(
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
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod error;
pub use error::{Result, SpringError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore error`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add springcore/src/error.rs springcore/src/lib.rs
git commit -m "feat(error): typed SpringError and Result alias

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: End types and geometry relations

**Files:**
- Create: `springcore/src/end_type.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `Length` (units), nothing else.
- Produces:
  - `enum EndType { Plain, PlainGround, Squared, SquaredGround }`
    (derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`).
  - `EndType::end_coils(self) -> f64`
  - `EndType::total_coils(self, active: f64) -> f64`
  - `EndType::active_coils(self, total: f64) -> f64`
  - `EndType::solid_length(self, wire_dia: Length, active: f64) -> Length`
  - `EndType::free_length(self, wire_dia: Length, active: f64, pitch: Length) -> Length`
  - `EndType::pitch_from_free_length(self, wire_dia: Length, active: f64, free_length: Length) -> Length`

All relations per **Shigley Table 10-1**. Values used:

| End type | Ne | Nt(Na) | Ls | L0 |
|---|---|---|---|---|
| Plain | 0 | Na | d(Nt+1) | pNa + d |
| PlainGround | 1 | Na+1 | dNt | p(Na+1) |
| Squared | 2 | Na+2 | d(Nt+1) | pNa + 3d |
| SquaredGround | 2 | Na+2 | dNt | pNa + 2d |

- [ ] **Step 1: Write the failing test**

In `springcore/src/end_type.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Length;
    use approx::assert_relative_eq;

    #[test]
    fn squared_ground_relations() {
        let e = EndType::SquaredGround;
        let d = Length::from_millimeters(2.0);
        let na = 8.0;
        assert_relative_eq!(e.total_coils(na), 10.0, max_relative = 1e-12);
        assert_relative_eq!(e.active_coils(10.0), 8.0, max_relative = 1e-12);
        // Solid length = d * Nt = 2 mm * 10 = 20 mm
        assert_relative_eq!(e.solid_length(d, na).millimeters(), 20.0, max_relative = 1e-12);
        // Free length = p*Na + 2d, with p = 5 mm: 40 + 4 = 44 mm
        let p = Length::from_millimeters(5.0);
        assert_relative_eq!(e.free_length(d, na, p).millimeters(), 44.0, max_relative = 1e-12);
        // Inverse: pitch from free length recovers 5 mm
        let l0 = Length::from_millimeters(44.0);
        assert_relative_eq!(e.pitch_from_free_length(d, na, l0).millimeters(), 5.0, max_relative = 1e-12);
    }

    #[test]
    fn plain_relations() {
        let e = EndType::Plain;
        let d = Length::from_millimeters(1.0);
        // Nt = Na; Ls = d(Nt+1)
        assert_relative_eq!(e.total_coils(10.0), 10.0, max_relative = 1e-12);
        assert_relative_eq!(e.solid_length(d, 10.0).millimeters(), 11.0, max_relative = 1e-12);
        // L0 = p*Na + d, p = 3 mm: 30 + 1 = 31 mm
        let p = Length::from_millimeters(3.0);
        assert_relative_eq!(e.free_length(d, 10.0, p).millimeters(), 31.0, max_relative = 1e-12);
    }

    #[test]
    fn plain_ground_free_length_uses_na_plus_one() {
        let e = EndType::PlainGround;
        let d = Length::from_millimeters(1.0);
        // L0 = p*(Na+1), p = 2 mm, Na = 9: 2*10 = 20 mm
        let p = Length::from_millimeters(2.0);
        assert_relative_eq!(e.free_length(d, 9.0, p).millimeters(), 20.0, max_relative = 1e-12);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore end_type`
Expected: FAIL — `EndType` not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/end_type.rs`:

```rust
//! End conditions for helical compression springs and the coil/length
//! relations they imply. All relations per Shigley Table 10-1.

use crate::units::Length;
use serde::{Deserialize, Serialize};

/// Spring end condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndType {
    Plain,
    PlainGround,
    Squared,
    SquaredGround,
}

impl EndType {
    /// Number of inactive end coils (Shigley Table 10-1).
    pub fn end_coils(self) -> f64 {
        match self {
            Self::Plain => 0.0,
            Self::PlainGround => 1.0,
            Self::Squared | Self::SquaredGround => 2.0,
        }
    }

    /// Total coils from active coils: Nt = Na + Ne (Shigley Table 10-1).
    pub fn total_coils(self, active: f64) -> f64 {
        active + self.end_coils()
    }

    /// Active coils from total coils: Na = Nt - Ne.
    pub fn active_coils(self, total: f64) -> f64 {
        total - self.end_coils()
    }

    /// Solid (fully compressed) length (Shigley Table 10-1).
    pub fn solid_length(self, wire_dia: Length, active: f64) -> Length {
        let d = wire_dia.meters();
        let nt = self.total_coils(active);
        let ls = match self {
            // Ground ends: Ls = d * Nt
            Self::PlainGround | Self::SquaredGround => d * nt,
            // Non-ground ends: Ls = d * (Nt + 1)
            Self::Plain | Self::Squared => d * (nt + 1.0),
        };
        Length::from_meters(ls)
    }

    /// Free length from pitch (Shigley Table 10-1).
    pub fn free_length(self, wire_dia: Length, active: f64, pitch: Length) -> Length {
        let d = wire_dia.meters();
        let p = pitch.meters();
        let l0 = match self {
            Self::Plain => p * active + d,
            Self::PlainGround => p * (active + 1.0),
            Self::Squared => p * active + 3.0 * d,
            Self::SquaredGround => p * active + 2.0 * d,
        };
        Length::from_meters(l0)
    }

    /// Pitch that yields a given free length (inverse of `free_length`).
    pub fn pitch_from_free_length(self, wire_dia: Length, active: f64, free_length: Length) -> Length {
        let d = wire_dia.meters();
        let l0 = free_length.meters();
        let p = match self {
            Self::Plain => (l0 - d) / active,
            Self::PlainGround => l0 / (active + 1.0),
            Self::Squared => (l0 - 3.0 * d) / active,
            Self::SquaredGround => (l0 - 2.0 * d) / active,
        };
        Length::from_meters(p)
    }
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod end_type;
pub use end_type::EndType;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore end_type`
Expected: PASS (3 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/end_type.rs springcore/src/lib.rs
git commit -m "feat(geometry): EndType coil and length relations (Shigley Table 10-1)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Mechanics — rate, stress, frequency, buckling

**Files:**
- Create: `springcore/src/mechanics.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `Length, Force, Stress, SpringRate, Frequency, MassDensity`.
- Produces:
  - `spring_index(mean_dia: Length, wire_dia: Length) -> f64`
  - `wahl_factor(index: f64) -> f64`
  - `bergstrasser_factor(index: f64) -> f64`
  - `spring_rate(shear_modulus: Stress, wire_dia: Length, mean_dia: Length, active: f64) -> SpringRate`
  - `active_coils_for_rate(shear_modulus: Stress, wire_dia: Length, mean_dia: Length, rate: SpringRate) -> f64`
  - `corrected_shear_stress(force: Force, mean_dia: Length, wire_dia: Length, factor: f64) -> Stress`
  - `natural_frequency(wire_dia: Length, mean_dia: Length, active: f64, shear_modulus: Stress, density: MassDensity) -> Frequency`
  - `enum EndFixity { FixedFixed, FixedPinned, PinnedPinned, FixedFree }` with `alpha(self) -> f64`
  - `critical_free_length(mean_dia: Length, youngs_modulus: Stress, shear_modulus: Stress, fixity: EndFixity) -> Length`
  - `is_buckling_stable(free_length: Length, mean_dia: Length, youngs_modulus: Stress, shear_modulus: Stress, fixity: EndFixity) -> bool`

- [ ] **Step 1: Write the failing test**

In `springcore/src/mechanics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Force, Length, MassDensity, SpringRate, Stress};
    use approx::assert_relative_eq;

    #[test]
    fn index_is_d_over_d() {
        let c = spring_index(Length::from_millimeters(10.0), Length::from_millimeters(1.0));
        assert_relative_eq!(c, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn wahl_factor_c10() {
        // Kw = (4C-1)/(4C-4) + 0.615/C; C=10 -> 39/36 + 0.0615
        assert_relative_eq!(wahl_factor(10.0), 39.0 / 36.0 + 0.0615, max_relative = 1e-12);
    }

    #[test]
    fn bergstrasser_factor_c10() {
        // Kb = (4C+2)/(4C-3); C=10 -> 42/37
        assert_relative_eq!(bergstrasser_factor(10.0), 42.0 / 37.0, max_relative = 1e-12);
    }

    #[test]
    fn rate_clean_case() {
        // k = G d^4 / (8 D^3 Na); G=80e9, d=1mm, D=10mm, Na=10 -> exactly 1000 N/m
        let k = spring_rate(
            Stress::from_pascals(80.0e9),
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            10.0,
        );
        assert_relative_eq!(k.newtons_per_meter(), 1000.0, max_relative = 1e-12);
    }

    #[test]
    fn active_coils_inverts_rate() {
        let na = active_coils_for_rate(
            Stress::from_pascals(80.0e9),
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            SpringRate::from_newtons_per_meter(1000.0),
        );
        assert_relative_eq!(na, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn corrected_stress_c10() {
        // tau = Kw * 8 F D / (pi d^3); F=10 N, D=10mm, d=1mm, Kw=39/36+0.0615
        let kw = 39.0 / 36.0 + 0.0615;
        let s = corrected_shear_stress(
            Force::from_newtons(10.0),
            Length::from_millimeters(10.0),
            Length::from_millimeters(1.0),
            kw,
        );
        let expected = kw * 8.0 * 10.0 * 0.010 / (std::f64::consts::PI * 0.001_f64.powi(3));
        assert_relative_eq!(s.pascals(), expected, max_relative = 1e-12);
    }

    #[test]
    fn natural_frequency_case() {
        // fn = (d/(2*pi*Na*D^2)) * sqrt(G/(32*rho))
        let f = natural_frequency(
            Length::from_millimeters(1.0),
            Length::from_millimeters(10.0),
            10.0,
            Stress::from_pascals(80.0e9),
            MassDensity::from_kg_per_m3(7850.0),
        );
        let expected = (0.001 / (2.0 * std::f64::consts::PI * 10.0 * 0.010_f64.powi(2)))
            * (80.0e9_f64 / (32.0 * 7850.0)).sqrt();
        assert_relative_eq!(f.hertz(), expected, max_relative = 1e-12);
    }

    #[test]
    fn buckling_critical_length() {
        // L0_cr = (pi D / alpha) * sqrt(2(E-G)/(2G+E)); E=200e9, G=80e9, fixed-fixed alpha=0.5
        let l = critical_free_length(
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed,
        );
        let expected = (std::f64::consts::PI * 0.010 / 0.5)
            * (2.0 * (200.0e9 - 80.0e9) / (2.0 * 80.0e9 + 200.0e9)).sqrt();
        assert_relative_eq!(l.meters(), expected, max_relative = 1e-12);
        // A spring shorter than critical is stable; far longer is not.
        assert!(is_buckling_stable(
            Length::from_meters(expected * 0.5),
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed
        ));
        assert!(!is_buckling_stable(
            Length::from_meters(expected * 2.0),
            Length::from_millimeters(10.0),
            Stress::from_pascals(200.0e9),
            Stress::from_pascals(80.0e9),
            EndFixity::FixedFixed
        ));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore mechanics`
Expected: FAIL — functions not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/mechanics.rs`:

```rust
//! Governing mechanics for cylindrical helical compression springs of round wire.
//! Each formula cites its source.

use crate::units::{Force, Frequency, Length, MassDensity, SpringRate, Stress};
use std::f64::consts::PI;

/// Spring index C = D/d (Shigley Eq. 10-1).
pub fn spring_index(mean_dia: Length, wire_dia: Length) -> f64 {
    mean_dia.meters() / wire_dia.meters()
}

/// Wahl curvature-and-shear correction factor (Wahl 1963; Shigley Eq. 10-5):
/// Kw = (4C-1)/(4C-4) + 0.615/C.
pub fn wahl_factor(index: f64) -> f64 {
    (4.0 * index - 1.0) / (4.0 * index - 4.0) + 0.615 / index
}

/// Bergsträsser correction factor (Shigley Eq. 10-6): Kb = (4C+2)/(4C-3).
pub fn bergstrasser_factor(index: f64) -> f64 {
    (4.0 * index + 2.0) / (4.0 * index - 3.0)
}

/// Spring rate k = G d^4 / (8 D^3 Na) (Shigley Eq. 10-9; EN 13906-1).
pub fn spring_rate(shear_modulus: Stress, wire_dia: Length, mean_dia: Length, active: f64) -> SpringRate {
    let g = shear_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    SpringRate::from_newtons_per_meter(g * d.powi(4) / (8.0 * dm.powi(3) * active))
}

/// Active coils required for a target rate (inverse of `spring_rate`).
pub fn active_coils_for_rate(
    shear_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    rate: SpringRate,
) -> f64 {
    let g = shear_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    g * d.powi(4) / (8.0 * dm.powi(3) * rate.newtons_per_meter())
}

/// Corrected shear stress tau = K * 8 F D / (pi d^3) (Shigley Eq. 10-7).
/// `factor` is the chosen correction factor (Wahl or Bergsträsser).
pub fn corrected_shear_stress(force: Force, mean_dia: Length, wire_dia: Length, factor: f64) -> Stress {
    let f = force.newtons();
    let dm = mean_dia.meters();
    let d = wire_dia.meters();
    Stress::from_pascals(factor * 8.0 * f * dm / (PI * d.powi(3)))
}

/// Natural frequency of a both-ends-fixed spring (Shigley Eq. 10-25),
/// fn = (d / (2*pi*Na*D^2)) * sqrt(G / (32*rho)), rho = mass density.
pub fn natural_frequency(
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    shear_modulus: Stress,
    density: MassDensity,
) -> Frequency {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let g = shear_modulus.pascals();
    let rho = density.kg_per_m3();
    let hz = (d / (2.0 * PI * active * dm.powi(2))) * (g / (32.0 * rho)).sqrt();
    Frequency::from_hertz(hz)
}

/// End-condition constant alpha for buckling (Shigley Table 10-2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndFixity {
    /// Both ends squared and ground, between parallel plates.
    FixedFixed,
    /// One end fixed, the other pivoted.
    FixedPinned,
    /// Both ends pivoted.
    PinnedPinned,
    /// One end fixed, the other free.
    FixedFree,
}

impl EndFixity {
    pub fn alpha(self) -> f64 {
        match self {
            Self::FixedFixed => 0.5,
            Self::FixedPinned => 0.707,
            Self::PinnedPinned => 1.0,
            Self::FixedFree => 2.0,
        }
    }
}

/// Critical free length for absolute stability (Shigley Eq. 10-10):
/// L0_cr = (pi*D/alpha) * sqrt(2(E-G)/(2G+E)).
/// A spring with L0 below this cannot buckle at any deflection (conservative; the
/// deflection-ratio refinement of Eq. 10-11 is deferred — see ADR 0005).
pub fn critical_free_length(
    mean_dia: Length,
    youngs_modulus: Stress,
    shear_modulus: Stress,
    fixity: EndFixity,
) -> Length {
    let dm = mean_dia.meters();
    let e = youngs_modulus.pascals();
    let g = shear_modulus.pascals();
    let l0 = (PI * dm / fixity.alpha()) * (2.0 * (e - g) / (2.0 * g + e)).sqrt();
    Length::from_meters(l0)
}

/// True when the spring's free length is at or below the absolute-stability limit.
pub fn is_buckling_stable(
    free_length: Length,
    mean_dia: Length,
    youngs_modulus: Stress,
    shear_modulus: Stress,
    fixity: EndFixity,
) -> bool {
    free_length.meters() <= critical_free_length(mean_dia, youngs_modulus, shear_modulus, fixity).meters()
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod mechanics;
pub use mechanics::EndFixity;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore mechanics`
Expected: PASS (8 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/mechanics.rs springcore/src/lib.rs
git commit -m "feat(mechanics): rate, Wahl/Bergstrasser, stress, frequency, buckling

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Numeric root-find kernel

**Files:**
- Create: `springcore/src/numeric.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `Result`, `SpringError` (error module).
- Produces:
  - `struct SolveConfig { pub x_tol: f64, pub f_tol: f64, pub max_iter: u32 }`
    with `impl Default` (`x_tol = 1e-12`, `f_tol = 1e-12`, `max_iter = 200`).
  - `find_root_bracketed<F: Fn(f64) -> f64>(f: F, lo: f64, hi: f64, cfg: SolveConfig) -> Result<f64>`
    — robust derivative-free bracketed solver (Illinois variant of false position).

- [ ] **Step 1: Write the failing test**

In `springcore/src/numeric.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use approx::assert_relative_eq;

    #[test]
    fn finds_sqrt2() {
        let r = find_root_bracketed(|x| x * x - 2.0, 0.0, 2.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, std::f64::consts::SQRT_2, max_relative = 1e-10);
    }

    #[test]
    fn finds_cube_root() {
        let r = find_root_bracketed(|x| x * x * x - 27.0, 0.0, 10.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, 3.0, max_relative = 1e-10);
    }

    #[test]
    fn rejects_bracket_without_sign_change() {
        let err = find_root_bracketed(|x| x * x - 2.0, 2.0, 3.0, SolveConfig::default()).unwrap_err();
        assert_eq!(err, SpringError::InvalidBracket);
    }

    #[test]
    fn reports_non_convergence() {
        let cfg = SolveConfig { x_tol: 1e-18, f_tol: 1e-18, max_iter: 1 };
        let err = find_root_bracketed(|x| x * x * x - 2.0, 0.0, 2.0, cfg).unwrap_err();
        assert_eq!(err, SpringError::NonConvergence { iterations: 1 });
    }

    #[test]
    fn detects_root_at_endpoint() {
        let r = find_root_bracketed(|x| x - 1.0, 1.0, 5.0, SolveConfig::default()).unwrap();
        assert_relative_eq!(r, 1.0, max_relative = 1e-12);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore numeric`
Expected: FAIL — `find_root_bracketed` not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/numeric.rs`:

```rust
//! Derivative-free bracketed root finder. Uses the Illinois variant of the
//! false-position (regula falsi) method, which is guaranteed to keep the root
//! bracketed and converges superlinearly.
//!
//! Reference: Dowell, M. & Jarratt, P. (1971), "A modified regula falsi method
//! for computing the root of an equation," BIT 11(2), 168–174.

use crate::error::{Result, SpringError};

/// Convergence configuration for [`find_root_bracketed`].
#[derive(Debug, Clone, Copy)]
pub struct SolveConfig {
    /// Stop when the bracket width is below this.
    pub x_tol: f64,
    /// Stop when |f(c)| is below this.
    pub f_tol: f64,
    /// Maximum iterations before reporting non-convergence.
    pub max_iter: u32,
}

impl Default for SolveConfig {
    fn default() -> Self {
        Self { x_tol: 1e-12, f_tol: 1e-12, max_iter: 200 }
    }
}

/// Find a root of `f` within `[lo, hi]`, which must bracket a sign change.
pub fn find_root_bracketed<F: Fn(f64) -> f64>(f: F, lo: f64, hi: f64, cfg: SolveConfig) -> Result<f64> {
    let (mut a, mut b) = (lo, hi);
    let (mut fa, mut fb) = (f(a), f(b));
    if fa == 0.0 {
        return Ok(a);
    }
    if fb == 0.0 {
        return Ok(b);
    }
    if fa.signum() == fb.signum() {
        return Err(SpringError::InvalidBracket);
    }
    // side tracks which endpoint was retained last, for the Illinois halving.
    let mut side: i8 = 0;
    for _ in 0..cfg.max_iter {
        let c = (a * fb - b * fa) / (fb - fa);
        let fc = f(c);
        if fc.abs() < cfg.f_tol || (b - a).abs() < cfg.x_tol {
            return Ok(c);
        }
        if fc.signum() == fb.signum() {
            b = c;
            fb = fc;
            if side == -1 {
                fa *= 0.5;
            }
            side = -1;
        } else {
            a = c;
            fa = fc;
            if side == 1 {
                fb *= 0.5;
            }
            side = 1;
        }
    }
    Err(SpringError::NonConvergence { iterations: cfg.max_iter })
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod numeric;
pub use numeric::{find_root_bracketed, SolveConfig};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore numeric`
Expected: PASS (5 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/numeric.rs springcore/src/lib.rs
git commit -m "feat(numeric): bracketed Illinois root-find kernel

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: Materials model and curated data file

**Files:**
- Create: `springcore/src/material.rs`
- Create: `springcore/data/materials.toml`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `Length, Stress, MassDensity` (units); `Result, SpringError`.
- Produces:
  - `enum MtsForm { Constant, PowerLaw, Polynomial }`
  - `enum StrengthUnits { UsKpsiInch, SiMpaMm }` with `length_native(self, Length) -> f64`
    and `stress_from_native(self, f64) -> Stress`
  - `struct MtsEquation { form, units, coefficients: Vec<f64>, valid_dia_min: Length, valid_dia_max: Length }`
    with `evaluate(&self, d: Length) -> Result<Stress>`
  - `struct Endurance { ssa: Stress, ssm: Stress, peened: bool }`
  - `struct Material { name, specification, mts, youngs_modulus, shear_modulus, density,
    allowable_pct_torsion, allowable_pct_bending, allowable_pct_set, endurance: Option<Endurance>, citations }`
    with `Material::min_tensile_strength(&self, d: Length) -> Result<Stress>`
  - `struct MaterialSet { ... }` with `from_toml_str(&str) -> Result<Self>`,
    `load_default() -> Self`, `get(&self, &str) -> Result<&Material>`, `names(&self) -> Vec<&str>`

**Correctness rule (ADR 0003):** strength coefficients are stored and evaluated in
their native units (`Sut = A/d^m`, A in kpsi·inᵐ or MPa·mmᵐ — Shigley Table 10-4);
only the scalar result is converted to SI. Never convert coefficients.

- [ ] **Step 1: Write the failing test**

In `springcore/src/material.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpringError;
    use crate::units::Length;
    use approx::assert_relative_eq;

    const SAMPLE: &str = r#"
[[material]]
name = "Test Music Wire"
specification = "ASTM A228"
citations = "Shigley Table 10-4 (A, m); Table 10-5 (E, G)"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [2211.0, 0.145]
valid_dia_min_mm = 0.10
valid_dia_max_mm = 6.5
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 80.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
[material.endurance]
ssa_mpa = 241.0
ssm_mpa = 379.0
peened = false
"#;

    #[test]
    fn power_law_mts_si_native() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        // Sut = 2211 / d^0.145, d in mm. At d=1mm -> 2211 MPa.
        assert_relative_eq!(m.min_tensile_strength(Length::from_millimeters(1.0)).unwrap().megapascals(), 2211.0, max_relative = 1e-9);
        // At d=2mm -> 2211 / 2^0.145
        let expected = 2211.0 / 2.0_f64.powf(0.145);
        assert_relative_eq!(m.min_tensile_strength(Length::from_millimeters(2.0)).unwrap().megapascals(), expected, max_relative = 1e-9);
    }

    #[test]
    fn us_native_units_not_converted_as_coefficients() {
        let us = r#"
[[material]]
name = "US Music Wire"
specification = "ASTM A228"
citations = "Shigley Table 10-4"
mts_form = "power_law"
mts_units = "us_kpsi_inch"
mts_coefficients = [201.0, 0.145]
valid_dia_min_mm = 2.54
valid_dia_max_mm = 25.4
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 80.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
"#;
        let set = MaterialSet::from_toml_str(us).unwrap();
        let m = set.get("US Music Wire").unwrap();
        // d = 0.2 in. Sut = 201 / 0.2^0.145 kpsi, evaluated in inches.
        let d = Length::from_inches(0.2);
        let expected_kpsi = 201.0 / 0.2_f64.powf(0.145);
        let got = m.min_tensile_strength(d).unwrap();
        assert_relative_eq!(got.psi() / 1000.0, expected_kpsi, max_relative = 1e-9);
    }

    #[test]
    fn out_of_range_diameter_rejected() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        let err = m.min_tensile_strength(Length::from_millimeters(10.0)).unwrap_err();
        assert!(matches!(err, SpringError::DiameterOutOfRange { .. }));
    }

    #[test]
    fn endurance_optional() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        let m = set.get("Test Music Wire").unwrap();
        assert!(m.endurance.is_some());
    }

    #[test]
    fn missing_material_errors() {
        let set = MaterialSet::from_toml_str(SAMPLE).unwrap();
        assert_eq!(set.get("nope").unwrap_err(), SpringError::MaterialNotFound("nope".into()));
    }

    #[test]
    fn default_set_loads_four_materials_with_music_wire() {
        let set = MaterialSet::load_default();
        assert!(set.names().contains(&"Music Wire"));
        // Music wire at 1 mm -> 2211 MPa (Shigley Table 10-4).
        let m = set.get("Music Wire").unwrap();
        assert_relative_eq!(m.min_tensile_strength(Length::from_millimeters(1.0)).unwrap().megapascals(), 2211.0, max_relative = 1e-9);
        assert_eq!(set.names().len(), 4);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore material`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/material.rs`:

```rust
//! Spring-wire materials. Strength is defined by a diameter-dependent equation
//! whose coefficients live in their native unit system (Shigley Table 10-4);
//! only the scalar result is converted to SI (see ADR 0003).

use crate::error::{Result, SpringError};
use crate::units::{Length, MassDensity, Stress};
use serde::Deserialize;

const PSI_PER_KPSI: f64 = 1000.0;

/// Functional form of the minimum-tensile-strength equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtsForm {
    /// Sut = c0 (constant).
    Constant,
    /// Sut = A / d^m, coefficients = [A, m] (Shigley Eq. 10-14).
    PowerLaw,
    /// Sut = sum_i c_i d^i, coefficients = [c0, c1, ...].
    Polynomial,
}

/// Native unit system of an MTS equation's coefficients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthUnits {
    /// A in kpsi·inᵐ, diameter in inches, result in kpsi.
    UsKpsiInch,
    /// A in MPa·mmᵐ, diameter in mm, result in MPa.
    SiMpaMm,
}

impl StrengthUnits {
    /// Express a diameter in this system's native length unit.
    pub fn length_native(self, d: Length) -> f64 {
        match self {
            Self::UsKpsiInch => d.inches(),
            Self::SiMpaMm => d.millimeters(),
        }
    }

    /// Convert a native strength scalar to SI stress.
    pub fn stress_from_native(self, value: f64) -> Stress {
        match self {
            Self::UsKpsiInch => Stress::from_psi(value * PSI_PER_KPSI),
            Self::SiMpaMm => Stress::from_megapascals(value),
        }
    }
}

/// Diameter-dependent minimum tensile strength.
#[derive(Debug, Clone)]
pub struct MtsEquation {
    pub form: MtsForm,
    pub units: StrengthUnits,
    pub coefficients: Vec<f64>,
    pub valid_dia_min: Length,
    pub valid_dia_max: Length,
}

impl MtsEquation {
    /// Minimum tensile strength at diameter `d`, in SI.
    pub fn evaluate(&self, d: Length) -> Result<Stress> {
        if d.meters() < self.valid_dia_min.meters() || d.meters() > self.valid_dia_max.meters() {
            return Err(SpringError::DiameterOutOfRange {
                diameter_m: d.meters(),
                min_m: self.valid_dia_min.meters(),
                max_m: self.valid_dia_max.meters(),
            });
        }
        let dn = self.units.length_native(d);
        let c = &self.coefficients;
        let raw = match self.form {
            MtsForm::Constant => c[0],
            MtsForm::PowerLaw => c[0] / dn.powf(c[1]),
            MtsForm::Polynomial => c.iter().enumerate().map(|(i, ci)| ci * dn.powi(i as i32)).sum(),
        };
        Ok(self.units.stress_from_native(raw))
    }
}

/// Cited endurance data (Zimmerli; steel spring wire only).
#[derive(Debug, Clone, Copy)]
pub struct Endurance {
    /// Alternating shear endurance strength.
    pub ssa: Stress,
    /// Mean shear endurance strength.
    pub ssm: Stress,
    /// Whether the data is for shot-peened springs.
    pub peened: bool,
}

/// A spring-wire material.
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub specification: String,
    pub mts: MtsEquation,
    pub youngs_modulus: Stress,
    pub shear_modulus: Stress,
    pub density: MassDensity,
    pub allowable_pct_torsion: f64,
    pub allowable_pct_bending: f64,
    pub allowable_pct_set: f64,
    pub endurance: Option<Endurance>,
    pub citations: String,
}

impl Material {
    pub fn min_tensile_strength(&self, d: Length) -> Result<Stress> {
        self.mts.evaluate(d)
    }
}

/// An immutable, named collection of materials.
#[derive(Debug, Clone)]
pub struct MaterialSet {
    materials: Vec<Material>,
}

impl MaterialSet {
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let raw: RawDoc = toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))?;
        let materials = raw.material.into_iter().map(Material::from).collect();
        Ok(Self { materials })
    }

    /// Load the curated material set bundled with the crate.
    pub fn load_default() -> Self {
        Self::from_toml_str(include_str!("../data/materials.toml"))
            .expect("bundled materials.toml is valid")
    }

    pub fn get(&self, name: &str) -> Result<&Material> {
        self.materials
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| SpringError::MaterialNotFound(name.to_string()))
    }

    pub fn names(&self) -> Vec<&str> {
        self.materials.iter().map(|m| m.name.as_str()).collect()
    }
}

// --- TOML deserialization layer (native, human-readable units) ---

#[derive(Deserialize)]
struct RawDoc {
    material: Vec<RawMaterial>,
}

#[derive(Deserialize)]
struct RawMaterial {
    name: String,
    specification: String,
    citations: String,
    mts_form: String,
    mts_units: String,
    mts_coefficients: Vec<f64>,
    valid_dia_min_mm: f64,
    valid_dia_max_mm: f64,
    youngs_modulus_gpa: f64,
    shear_modulus_gpa: f64,
    density_kg_per_m3: f64,
    allowable_pct_torsion: f64,
    allowable_pct_bending: f64,
    allowable_pct_set: f64,
    endurance: Option<RawEndurance>,
}

#[derive(Deserialize)]
struct RawEndurance {
    ssa_mpa: f64,
    ssm_mpa: f64,
    peened: bool,
}

impl From<RawMaterial> for Material {
    fn from(r: RawMaterial) -> Self {
        let form = match r.mts_form.as_str() {
            "constant" => MtsForm::Constant,
            "power_law" => MtsForm::PowerLaw,
            "polynomial" => MtsForm::Polynomial,
            other => panic!("unknown mts_form: {other}"),
        };
        let units = match r.mts_units.as_str() {
            "us_kpsi_inch" => StrengthUnits::UsKpsiInch,
            "si_mpa_mm" => StrengthUnits::SiMpaMm,
            other => panic!("unknown mts_units: {other}"),
        };
        Material {
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
            citations: r.citations,
        }
    }
}
```

(The `panic!` arms in `From` are acceptable only because they are unreachable for the
bundled file; `from_toml_str` callers with untrusted input should be revisited if an
editable database is added in a later sub-project. Note this in the function doc.)

- [ ] **Step 4: Create the curated data file**

`springcore/data/materials.toml` — all values cited inline. Coefficients are Shigley
Table 10-4 (A in MPa·mmᵐ, m dimensionless); moduli/density Shigley Table 10-5;
endurance Zimmerli per Shigley §10-9; allowable percentages follow SMI Handbook
design-stress guidance.

```toml
# Curated spring-wire materials. Strength coefficients are in native units
# (MPa·mm^m) per Shigley Table 10-4 and are evaluated in those units.
# Sources: Shigley's Mechanical Engineering Design (10th ed.) Tables 10-4, 10-5,
# §10-9 (Zimmerli endurance); SMI Handbook of Spring Design (design stresses).

[[material]]
name = "Music Wire"
specification = "ASTM A228"
citations = "Shigley Table 10-4 (A=2211 MPa*mm^m, m=0.145); Table 10-5 (E, G); Zimmerli via Shigley 10-9; SMI design stresses"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [2211.0, 0.145]
valid_dia_min_mm = 0.10
valid_dia_max_mm = 6.5
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 80.0
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.60
[material.endurance]
ssa_mpa = 241.0
ssm_mpa = 379.0
peened = false

[[material]]
name = "Oil-Tempered Wire"
specification = "ASTM A229"
citations = "Shigley Table 10-4 (A=1855 MPa*mm^m, m=0.187); Table 10-5 (E, G); Zimmerli via Shigley 10-9; SMI design stresses"
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [1855.0, 0.187]
valid_dia_min_mm = 0.50
valid_dia_max_mm = 12.7
youngs_modulus_gpa = 196.5
shear_modulus_gpa = 78.6
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.49
[material.endurance]
ssa_mpa = 241.0
ssm_mpa = 379.0
peened = false

[[material]]
name = "Stainless 302"
specification = "ASTM A313 (Type 302)"
citations = "Shigley Table 10-4 (A=1867 MPa*mm^m, m=0.146); Table 10-5 (E, G); SMI design stresses. No cited endurance data."
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [1867.0, 0.146]
valid_dia_min_mm = 0.30
valid_dia_max_mm = 6.5
youngs_modulus_gpa = 193.0
shear_modulus_gpa = 69.0
density_kg_per_m3 = 7920.0
allowable_pct_torsion = 0.35
allowable_pct_bending = 0.55
allowable_pct_set = 0.35

[[material]]
name = "Chrome-Silicon"
specification = "ASTM A401"
citations = "Shigley Table 10-4 (A=1974 MPa*mm^m, m=0.108); Table 10-5 (E, G); SMI design stresses. No cited endurance data in v1."
mts_form = "power_law"
mts_units = "si_mpa_mm"
mts_coefficients = [1974.0, 0.108]
valid_dia_min_mm = 1.6
valid_dia_max_mm = 9.5
youngs_modulus_gpa = 203.4
shear_modulus_gpa = 77.2
density_kg_per_m3 = 7850.0
allowable_pct_torsion = 0.45
allowable_pct_bending = 0.75
allowable_pct_set = 0.49
```

- [ ] **Step 5: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod material;
pub use material::{Endurance, Material, MaterialSet, MtsEquation, MtsForm, StrengthUnits};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p springcore material`
Expected: PASS (6 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/material.rs springcore/data/materials.toml springcore/src/lib.rs
git commit -m "feat(material): curated materials with unit-native MTS coefficients

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Forward design solve and design status

**Files:**
- Create: `springcore/src/design.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: units, `Material`, `EndType`, `EndFixity`, and all `mechanics` functions.
- Produces:
  - `struct LoadPoint { force: Force, deflection: Length, length: Length, shear_stress: Stress, pct_mts: f64 }`
  - `struct SpringDesign { wire_dia, mean_dia, index, active_coils, total_coils, rate,
    free_length, solid_length, pitch, outer_dia, inner_dia, min_tensile_strength,
    natural_frequency, buckling_stable, load_points: Vec<LoadPoint>, at_solid: LoadPoint, end_type }`
  - `solve_forward(material: &Material, end_type: EndType, fixity: EndFixity, wire_dia: Length,
    mean_dia: Length, active: f64, free_length: Length, loads: &[Force]) -> Result<SpringDesign>`
  - `enum Severity { Info, Caution, Warning }`
  - `struct StatusMessage { severity: Severity, message: String }`
  - `struct DesignStatus { messages: Vec<StatusMessage> }` with `has_warnings(&self) -> bool`
  - `evaluate_status(design: &SpringDesign, material: &Material) -> DesignStatus`

- [ ] **Step 1: Write the failing test**

In `springcore/src/design.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;

    fn music_wire() -> crate::material::Material {
        MaterialSet::load_default().get("Music Wire").unwrap().clone()
    }

    #[test]
    fn forward_solve_clean_case() {
        let m = music_wire();
        // d=2mm, D=20mm -> C=10, Na=10. G=80 GPa -> k = 2000 N/m.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
        )
        .unwrap();
        assert_relative_eq!(design.index, 10.0, max_relative = 1e-12);
        assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(design.total_coils, 12.0, max_relative = 1e-12);
        // Solid length = d*Nt = 2*12 = 24 mm
        assert_relative_eq!(design.solid_length.millimeters(), 24.0, max_relative = 1e-9);
        // Load 10 N -> deflection 10/2000 = 0.005 m = 5 mm
        let lp = &design.load_points[0];
        assert_relative_eq!(lp.deflection.millimeters(), 5.0, max_relative = 1e-9);
        // stress = Kw*8FD/(pi d^3), Kw = wahl(10)
        let kw = 39.0 / 36.0 + 0.0615;
        let expected = kw * 8.0 * 10.0 * 0.020 / (std::f64::consts::PI * 0.002_f64.powi(3));
        assert_relative_eq!(lp.shear_stress.pascals(), expected, max_relative = 1e-9);
    }

    #[test]
    fn status_flags_low_index() {
        let m = music_wire();
        // C = 16/2 = 8 is fine; make C=3 (D=6mm,d=2mm) to trigger low-index caution.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(2.0),
            Length::from_millimeters(6.0),
            10.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(10.0)],
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status.messages.iter().any(|msg| msg.message.contains("index")));
    }

    #[test]
    fn status_flags_overstress_at_solid() {
        let m = music_wire();
        // Very stiff, large deflection to solid -> overstress.
        let design = solve_forward(
            &m,
            EndType::SquaredGround,
            EndFixity::FixedFixed,
            Length::from_millimeters(1.0),
            Length::from_millimeters(8.0),
            6.0,
            Length::from_millimeters(60.0),
            &[Force::from_newtons(5.0)],
        )
        .unwrap();
        let status = evaluate_status(&design, &m);
        assert!(status.has_warnings());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore design`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/design.rs`:

```rust
//! Aggregate forward solve: from fully-determined geometry to a complete design,
//! plus engineering status checks. Formula sources cited at each call site.

use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{
    corrected_shear_stress, is_buckling_stable, natural_frequency, spring_index, spring_rate,
    wahl_factor, EndFixity,
};
use crate::units::{Force, Frequency, Length, SpringRate, Stress};
use crate::Result;

/// State of the spring at one axial load.
#[derive(Debug, Clone, Copy)]
pub struct LoadPoint {
    pub force: Force,
    pub deflection: Length,
    pub length: Length,
    pub shear_stress: Stress,
    pub pct_mts: f64,
}

/// A fully computed compression-spring design.
#[derive(Debug, Clone)]
pub struct SpringDesign {
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub index: f64,
    pub active_coils: f64,
    pub total_coils: f64,
    pub rate: SpringRate,
    pub free_length: Length,
    pub solid_length: Length,
    pub pitch: Length,
    pub outer_dia: Length,
    pub inner_dia: Length,
    pub min_tensile_strength: Stress,
    pub natural_frequency: Frequency,
    pub buckling_stable: bool,
    pub load_points: Vec<LoadPoint>,
    pub at_solid: LoadPoint,
    pub end_type: EndType,
}

fn load_point(force: Force, rate: SpringRate, free_length: Length, mean_dia: Length, wire_dia: Length, index: f64, mts: Stress) -> LoadPoint {
    // Deflection y = F/k (Shigley Eq. 10-9 rearranged).
    let y = force.newtons() / rate.newtons_per_meter();
    let length = Length::from_meters(free_length.meters() - y);
    let stress = corrected_shear_stress(force, mean_dia, wire_dia, wahl_factor(index));
    LoadPoint {
        force,
        deflection: Length::from_meters(y),
        length,
        shear_stress: stress,
        pct_mts: stress.pascals() / mts.pascals(),
    }
}

/// Compute a complete design from determined geometry plus operating loads.
#[allow(clippy::too_many_arguments)]
pub fn solve_forward(
    material: &Material,
    end_type: EndType,
    fixity: EndFixity,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    free_length: Length,
    loads: &[Force],
) -> Result<SpringDesign> {
    let index = spring_index(mean_dia, wire_dia);
    let rate = spring_rate(material.shear_modulus, wire_dia, mean_dia, active);
    let total_coils = end_type.total_coils(active);
    let solid_length = end_type.solid_length(wire_dia, active);
    let pitch = end_type.pitch_from_free_length(wire_dia, active, free_length);
    let mts = material.min_tensile_strength(wire_dia)?;
    let nat_freq = natural_frequency(wire_dia, mean_dia, active, material.shear_modulus, material.density);
    let stable = is_buckling_stable(free_length, mean_dia, material.youngs_modulus, material.shear_modulus, fixity);

    let load_points = loads
        .iter()
        .map(|&f| load_point(f, rate, free_length, mean_dia, wire_dia, index, mts))
        .collect();

    // Force required to reach solid: F = k * (L0 - Ls).
    let solid_force = Force::from_newtons(rate.newtons_per_meter() * (free_length.meters() - solid_length.meters()));
    let at_solid = load_point(solid_force, rate, free_length, mean_dia, wire_dia, index, mts);

    Ok(SpringDesign {
        wire_dia,
        mean_dia,
        index,
        active_coils: active,
        total_coils,
        rate,
        free_length,
        solid_length,
        pitch,
        outer_dia: Length::from_meters(mean_dia.meters() + wire_dia.meters()),
        inner_dia: Length::from_meters(mean_dia.meters() - wire_dia.meters()),
        min_tensile_strength: mts,
        natural_frequency: nat_freq,
        buckling_stable: stable,
        load_points,
        at_solid,
        end_type,
    })
}

/// Severity of a design-status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Caution,
    Warning,
}

/// One status/advisory message about a design.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub severity: Severity,
    pub message: String,
}

/// Collected status messages for a design.
#[derive(Debug, Clone, Default)]
pub struct DesignStatus {
    pub messages: Vec<StatusMessage>,
}

impl DesignStatus {
    pub fn has_warnings(&self) -> bool {
        self.messages.iter().any(|m| m.severity == Severity::Warning)
    }
}

/// Recommended spring-index bounds (SMI Handbook; Shigley §10-2 guidance).
const INDEX_MIN: f64 = 4.0;
const INDEX_MAX: f64 = 12.0;

/// Apply engineering checks to a computed design.
pub fn evaluate_status(design: &SpringDesign, material: &Material) -> DesignStatus {
    let mut messages = Vec::new();

    // Spring index outside the practical manufacturing range (SMI; Shigley §10-2).
    if design.index < INDEX_MIN || design.index > INDEX_MAX {
        messages.push(StatusMessage {
            severity: Severity::Caution,
            message: format!(
                "spring index {:.2} is outside the recommended range {INDEX_MIN}–{INDEX_MAX}",
                design.index
            ),
        });
    }

    // Operating stress above the allowable fraction of MTS (SMI design stress).
    let allowable = material.allowable_pct_torsion;
    for (i, lp) in design.load_points.iter().enumerate() {
        if lp.pct_mts > allowable {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: format!(
                    "load point {} stress is {:.1}% of MTS, above the allowable {:.0}%",
                    i + 1,
                    lp.pct_mts * 100.0,
                    allowable * 100.0
                ),
            });
        }
    }

    // Stress at solid above the set-allowable fraction (SMI).
    if design.at_solid.pct_mts > material.allowable_pct_set {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: format!(
                "stress at solid is {:.1}% of MTS, above the set allowable {:.0}%",
                design.at_solid.pct_mts * 100.0,
                material.allowable_pct_set * 100.0
            ),
        });
    }

    // Buckling (Shigley Eq. 10-10 absolute-stability criterion).
    if !design.buckling_stable {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "free length exceeds the absolute-stability limit; buckling possible".into(),
        });
    }

    // Free length shorter than solid length is physically invalid.
    if design.free_length.meters() < design.solid_length.meters() {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "free length is less than solid length".into(),
        });
    }

    DesignStatus { messages }
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod design;
pub use design::{
    evaluate_status, solve_forward, DesignStatus, LoadPoint, Severity, SpringDesign, StatusMessage,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore design`
Expected: PASS (3 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/design.rs springcore/src/lib.rs
git commit -m "feat(design): forward solve aggregate and engineering status checks

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Determined solve scenarios

**Files:**
- Create: `springcore/src/scenario.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: units, `Material`, `EndType`, `EndFixity`, `solve_forward`, `SpringDesign`,
  `mechanics::active_coils_for_rate`, `Result`, `SpringError`.
- Produces:
  - `trait Scenario { fn solve(&self, material: &Material) -> Result<SpringDesign>; }`
  - `struct PowerUser { end_type, fixity, wire_dia, mean_dia, active, free_length, loads: Vec<Force> }`
  - `struct TwoLoad { end_type, fixity, wire_dia, mean_dia, point1: (Force, Length), point2: (Force, Length) }`
  - `struct RateBased { end_type, fixity, wire_dia, mean_dia, rate: SpringRate, free_length, loads: Vec<Force> }`
  - `struct Dimensional { end_type, fixity, wire_dia, outer_dia, active, free_length, loads: Vec<Force> }`
  - each implements `Scenario`.

- [ ] **Step 1: Write the failing test**

In `springcore/src/scenario.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    fn music_wire() -> crate::material::Material {
        MaterialSet::load_default().get("Music Wire").unwrap().clone()
    }

    #[test]
    fn power_user_passes_through() {
        let s = PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s.solve(&music_wire()).unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
    }

    #[test]
    fn two_load_recovers_rate_and_free_length() {
        // From the clean case: k=2000 N/m, L0=60mm. Points: (10N,55mm),(20N,50mm).
        let s = TwoLoad {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            point1: (Force::from_newtons(10.0), Length::from_millimeters(55.0)),
            point2: (Force::from_newtons(20.0), Length::from_millimeters(50.0)),
        };
        let d = s.solve(&music_wire()).unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-9);
        assert_relative_eq!(d.free_length.millimeters(), 60.0, max_relative = 1e-9);
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
    }

    #[test]
    fn two_load_rejects_inconsistent_points() {
        // Higher force at longer length is impossible for a compression spring.
        let s = TwoLoad {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            point1: (Force::from_newtons(20.0), Length::from_millimeters(55.0)),
            point2: (Force::from_newtons(10.0), Length::from_millimeters(50.0)),
        };
        assert!(matches!(s.solve(&music_wire()), Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rate_based_hits_target_rate() {
        let s = RateBased {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            rate: SpringRate::from_newtons_per_meter(2000.0),
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s.solve(&music_wire()).unwrap();
        assert_relative_eq!(d.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
        assert_relative_eq!(d.active_coils, 10.0, max_relative = 1e-6);
    }

    #[test]
    fn dimensional_uses_outer_diameter() {
        // OD = 22mm, d = 2mm -> mean = 20mm -> C = 10.
        let s = Dimensional {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            outer_dia: Length::from_millimeters(22.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0)],
        };
        let d = s.solve(&music_wire()).unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-9);
        assert_relative_eq!(d.mean_dia.millimeters(), 20.0, max_relative = 1e-9);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore scenario`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/scenario.rs`:

```rust
//! Determined (closed-form) solve scenarios. Each scenario derives the four
//! geometry unknowns (d, D, Na, L0) from its inputs, then delegates to
//! `design::solve_forward`.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, EndFixity};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};

/// A solve scenario: a particular fixed assignment of which quantities are inputs.
pub trait Scenario {
    fn solve(&self, material: &Material) -> Result<SpringDesign>;
}

/// All geometry given; compute performance.
pub struct PowerUser {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for PowerUser {
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            self.active,
            self.free_length,
            &self.loads,
        )
    }
}

/// Two (force, length) operating points; solve rate and free length.
pub struct TwoLoad {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub point1: (Force, Length),
    pub point2: (Force, Length),
}

impl Scenario for TwoLoad {
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        let (f1, l1) = self.point1;
        let (f2, l2) = self.point2;
        let df = f2.newtons() - f1.newtons();
        let dl = l1.meters() - l2.meters();
        // A valid compression pair has more force at the shorter length.
        if dl <= 0.0 || df <= 0.0 {
            return Err(SpringError::InconsistentInputs(
                "two load points must show increasing force with decreasing length".into(),
            ));
        }
        let rate = SpringRate::from_newtons_per_meter(df / dl);
        // Free length: F1 = k (L0 - L1)  ->  L0 = L1 + F1/k.
        let free_length = Length::from_meters(l1.meters() + f1.newtons() / rate.newtons_per_meter());
        let active = active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, rate);
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            active,
            free_length,
            &[f1, f2],
        )
    }
}

/// Target rate given; solve active coils.
pub struct RateBased {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub mean_dia: Length,
    pub rate: SpringRate,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for RateBased {
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        let active = active_coils_for_rate(material.shear_modulus, self.wire_dia, self.mean_dia, self.rate);
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            self.mean_dia,
            active,
            self.free_length,
            &self.loads,
        )
    }
}

/// Outer diameter given; derive mean diameter.
pub struct Dimensional {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub wire_dia: Length,
    pub outer_dia: Length,
    pub active: f64,
    pub free_length: Length,
    pub loads: Vec<Force>,
}

impl Scenario for Dimensional {
    fn solve(&self, material: &Material) -> Result<SpringDesign> {
        let mean = Length::from_meters(self.outer_dia.meters() - self.wire_dia.meters());
        solve_forward(
            material,
            self.end_type,
            self.fixity,
            self.wire_dia,
            mean,
            self.active,
            self.free_length,
            &self.loads,
        )
    }
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod scenario;
pub use scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore scenario`
Expected: PASS (5 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/scenario.rs springcore/src/lib.rs
git commit -m "feat(scenario): four determined closed-form solve scenarios

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Minimum-weight optimization

**Files:**
- Create: `springcore/src/optimize.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: units, `Material`, `EndType`, `EndFixity`, `solve_forward`, `SpringDesign`,
  `mechanics` (`spring_index`, `wahl_factor`, `corrected_shear_stress`,
  `active_coils_for_rate`), `find_root_bracketed`, `SolveConfig`, `Result`, `SpringError`.
- Produces:
  - `enum BindingConstraint { Stress, Index, OuterDiameter }`
  - `struct MinWeightRequest { end_type, fixity, required_rate: SpringRate, max_force: Force,
    index_bounds: (f64, f64), max_outer_dia: Option<Length>, candidate_diameters: Vec<Length>,
    clash_allowance: f64 }`
  - `struct MinWeightSolution { design: SpringDesign, binding: BindingConstraint, mass_kg: f64 }`
  - `solve_min_weight(material: &Material, req: &MinWeightRequest) -> Result<MinWeightSolution>`

**Method (Shigley §10-11 spring-design approach; figure of merit = wire mass):** for each
candidate wire diameter, push mean diameter to the largest value allowed by the shear-stress
allowable and the index ceiling (mass ∝ `d²·D·Nt` but `Na ∝ d⁴/D³`, so net mass falls as `D`
grows — the optimum sits on the stress or index limit), honor an optional outer-diameter cap,
size active coils to the required rate, set free length to reach `max_force` with a clash
allowance (SMI ~10–15% clearance), then pick the minimum-mass feasible design.

- [ ] **Step 1: Write the failing test**

In `springcore/src/optimize.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{Material, MaterialSet};
    use crate::mechanics::EndFixity;
    use crate::units::{Force, Length, SpringRate};
    use approx::assert_relative_eq;

    fn music_wire() -> Material {
        MaterialSet::load_default().get("Music Wire").unwrap().clone()
    }

    fn base_request(candidates: Vec<f64>) -> MinWeightRequest {
        MinWeightRequest {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            required_rate: SpringRate::from_newtons_per_meter(2000.0),
            max_force: Force::from_newtons(50.0),
            index_bounds: (4.0, 12.0),
            max_outer_dia: None,
            candidate_diameters: candidates.into_iter().map(Length::from_millimeters).collect(),
            clash_allowance: 0.15,
        }
    }

    #[test]
    fn solution_is_feasible() {
        let m = music_wire();
        let sol = solve_min_weight(&m, &base_request(vec![1.5, 2.0, 2.5, 3.0])).unwrap();
        // Rate met.
        assert_relative_eq!(sol.design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
        // Stress at the operating load within allowable.
        let allowable = m.allowable_pct_torsion;
        assert!(sol.design.load_points[0].pct_mts <= allowable + 1e-6);
        // Index within bounds.
        assert!(sol.design.index >= 4.0 - 1e-9 && sol.design.index <= 12.0 + 1e-9);
        assert!(sol.mass_kg > 0.0);
    }

    #[test]
    fn picks_global_minimum_over_candidates() {
        let m = music_wire();
        let candidates = vec![1.5, 2.0, 2.5, 3.0];
        // Per-candidate mass via the same function restricted to one diameter.
        let per: Vec<f64> = candidates
            .iter()
            .filter_map(|&d| solve_min_weight(&m, &base_request(vec![d])).ok().map(|s| s.mass_kg))
            .collect();
        let best = solve_min_weight(&m, &base_request(candidates)).unwrap();
        let min = per.iter().cloned().fold(f64::INFINITY, f64::min);
        assert_relative_eq!(best.mass_kg, min, max_relative = 1e-9);
    }

    #[test]
    fn infeasible_when_outer_diameter_too_small() {
        let m = music_wire();
        let mut req = base_request(vec![1.5, 2.0, 2.5]);
        req.max_outer_dia = Some(Length::from_millimeters(3.0)); // forces index < 4
        assert!(matches!(solve_min_weight(&m, &req), Err(SpringError::Infeasible(_))));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore optimize`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/optimize.rs`:

```rust
//! Minimum-weight constrained optimization for compression springs.
//! Figure of merit = wire mass (Shigley §10-11). The optimum mean diameter for a
//! given wire size lies on the binding stress or index constraint.

use crate::design::{solve_forward, SpringDesign};
use crate::end_type::EndType;
use crate::material::Material;
use crate::mechanics::{active_coils_for_rate, corrected_shear_stress, wahl_factor, EndFixity};
use crate::numeric::{find_root_bracketed, SolveConfig};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};
use std::f64::consts::PI;

/// Which constraint limits the chosen design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingConstraint {
    Stress,
    Index,
    OuterDiameter,
}

/// A minimum-weight design problem.
#[derive(Debug, Clone)]
pub struct MinWeightRequest {
    pub end_type: EndType,
    pub fixity: EndFixity,
    pub required_rate: SpringRate,
    pub max_force: Force,
    pub index_bounds: (f64, f64),
    pub max_outer_dia: Option<Length>,
    pub candidate_diameters: Vec<Length>,
    /// Fractional clearance kept before solid at max force (SMI ~0.10–0.15).
    pub clash_allowance: f64,
}

/// The chosen design and why it is limited.
#[derive(Debug, Clone)]
pub struct MinWeightSolution {
    pub design: SpringDesign,
    pub binding: BindingConstraint,
    pub mass_kg: f64,
}

/// Wire mass of a design: rho * (pi^2/4) * d^2 * D * Nt (wire length ~ pi*D*Nt).
fn wire_mass(material: &Material, wire_dia: Length, mean_dia: Length, total_coils: f64) -> f64 {
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    material.density.kg_per_m3() * (PI * PI / 4.0) * d * d * dm * total_coils
}

/// Largest feasible mean diameter for a wire size, and which limit binds.
fn best_mean_dia(material: &Material, d: Length, max_force: Force, bounds: (f64, f64)) -> Option<(Length, BindingConstraint)> {
    let (c_min, c_max) = bounds;
    let allowable = material.allowable_pct_torsion * material.min_tensile_strength(d).ok()?.pascals();
    // Shear stress at max force as a function of mean diameter (monotonic increasing).
    let stress_at = |dm_m: f64| {
        let dm = Length::from_meters(dm_m);
        let c = dm_m / d.meters();
        corrected_shear_stress(max_force, dm, d, wahl_factor(c)).pascals()
    };
    let dm_lo = c_min * d.meters();
    let dm_hi = c_max * d.meters();
    // If even the smallest index overstresses, this wire size is infeasible.
    if stress_at(dm_lo) - allowable > 0.0 {
        return None;
    }
    // If the largest index is still under allowable, the index ceiling binds.
    if stress_at(dm_hi) - allowable <= 0.0 {
        return Some((Length::from_meters(dm_hi), BindingConstraint::Index));
    }
    // Otherwise the stress limit binds; solve for the mean diameter at allowable.
    let root = find_root_bracketed(|dm| stress_at(dm) - allowable, dm_lo, dm_hi, SolveConfig::default()).ok()?;
    Some((Length::from_meters(root), BindingConstraint::Stress))
}

/// Solve the minimum-weight problem.
pub fn solve_min_weight(material: &Material, req: &MinWeightRequest) -> Result<MinWeightSolution> {
    let (c_min, _c_max) = req.index_bounds;
    let mut best: Option<MinWeightSolution> = None;

    for &d in &req.candidate_diameters {
        let Some((mut mean, mut binding)) = best_mean_dia(material, d, req.max_force, req.index_bounds) else {
            continue;
        };
        // Apply an optional outer-diameter cap.
        if let Some(od_max) = req.max_outer_dia {
            if mean.meters() + d.meters() > od_max.meters() {
                let capped = od_max.meters() - d.meters();
                if capped / d.meters() < c_min {
                    continue; // capping would push index below the floor
                }
                mean = Length::from_meters(capped);
                binding = BindingConstraint::OuterDiameter;
            }
        }
        let active = active_coils_for_rate(material.shear_modulus, d, mean, req.required_rate);
        if active < 1.0 {
            continue; // fewer than one active coil is unphysical
        }
        let solid = req.end_type.solid_length(d, active);
        let travel = req.max_force.newtons() / req.required_rate.newtons_per_meter();
        let free_length = Length::from_meters(solid.meters() + travel * (1.0 + req.clash_allowance));
        let design = solve_forward(
            material,
            req.end_type,
            req.fixity,
            d,
            mean,
            active,
            free_length,
            &[req.max_force],
        )?;
        let mass = wire_mass(material, d, mean, design.total_coils);
        if best.as_ref().map(|b| mass < b.mass_kg).unwrap_or(true) {
            best = Some(MinWeightSolution { design, binding, mass_kg: mass });
        }
    }

    best.ok_or_else(|| SpringError::Infeasible("no candidate diameter satisfies the constraints".into()))
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod optimize;
pub use optimize::{solve_min_weight, BindingConstraint, MinWeightRequest, MinWeightSolution};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore optimize`
Expected: PASS (3 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/optimize.rs springcore/src/lib.rs
git commit -m "feat(optimize): minimum-weight constrained spring optimization

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Fatigue analysis

**Files:**
- Create: `springcore/src/fatigue.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: units, `Material`, `mechanics::{bergstrasser_factor, corrected_shear_stress, spring_index}`,
  `Result`, `SpringError`.
- Produces:
  - `struct FatigueResult { alternating_stress: Stress, mean_stress: Stress,
    fully_reversed_endurance: Stress, ultimate_shear: Stress, goodman_factor_of_safety: f64 }`
  - `analyze_fatigue(material: &Material, wire_dia: Length, mean_dia: Length, force_min: Force, force_max: Force) -> Result<FatigueResult>`
    returns `Err(SpringError::NoFatigueData(name))` when the material has no endurance data.

**Method (Shigley §10-9):** Bergsträsser-corrected alternating/mean shear stresses;
ultimate shear `Ssu = 0.67·Sut` (Eq. 10-30); fully-reversed endurance from Zimmerli data
via `Sse = Ssa/(1 − Ssm/Ssu)` (Eq. 10-31); Goodman factor of safety
`1/nf = τa/Sse + τm/Ssu`.

- [ ] **Step 1: Write the failing test**

In `springcore/src/fatigue.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{Material, MaterialSet};
    use crate::units::{Force, Length};
    use approx::assert_relative_eq;
    use std::f64::consts::PI;

    fn mat(name: &str) -> Material {
        MaterialSet::load_default().get(name).unwrap().clone()
    }

    #[test]
    fn goodman_safety_factor_music_wire() {
        let m = mat("Music Wire");
        let r = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
        )
        .unwrap();
        // Independent re-derivation per Shigley §10-9.
        let kb = 42.0 / 37.0; // Bergsträsser at C=10
        let d3 = 0.002_f64.powi(3);
        let ta = kb * 8.0 * 10.0 * 0.020 / (PI * d3); // Fa = 10 N
        let tm = kb * 8.0 * 20.0 * 0.020 / (PI * d3); // Fm = 20 N
        let sut = 2211.0e6 / 2.0_f64.powf(0.145);
        let ssu = 0.67 * sut;
        let sse = 241.0e6 / (1.0 - 379.0e6 / ssu);
        let nf = 1.0 / (ta / sse + tm / ssu);
        assert_relative_eq!(r.alternating_stress.pascals(), ta, max_relative = 1e-9);
        assert_relative_eq!(r.mean_stress.pascals(), tm, max_relative = 1e-9);
        assert_relative_eq!(r.goodman_factor_of_safety, nf, max_relative = 1e-9);
    }

    #[test]
    fn missing_endurance_degrades_gracefully() {
        let m = mat("Stainless 302");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(10.0),
            Force::from_newtons(30.0),
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::NoFatigueData(_)));
    }

    #[test]
    fn rejects_reversed_force_order() {
        let m = mat("Music Wire");
        let err = analyze_fatigue(
            &m,
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            Force::from_newtons(30.0),
            Force::from_newtons(10.0),
        )
        .unwrap_err();
        assert!(matches!(err, crate::SpringError::InconsistentInputs(_)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore fatigue`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/fatigue.rs`:

```rust
//! Fatigue analysis for compression springs (Shigley §10-9). Uses cited
//! per-material endurance data (Zimmerli); materials without it degrade
//! gracefully by returning `NoFatigueData`.

use crate::material::Material;
use crate::mechanics::{bergstrasser_factor, corrected_shear_stress, spring_index};
use crate::units::{Force, Length, Stress};
use crate::{Result, SpringError};

/// Ratio of ultimate shear strength to ultimate tensile strength (Shigley Eq. 10-30).
const SHEAR_TO_TENSILE: f64 = 0.67;

/// Result of a fatigue analysis over one load cycle.
#[derive(Debug, Clone, Copy)]
pub struct FatigueResult {
    pub alternating_stress: Stress,
    pub mean_stress: Stress,
    pub fully_reversed_endurance: Stress,
    pub ultimate_shear: Stress,
    pub goodman_factor_of_safety: f64,
}

/// Analyze fatigue for a spring cycling between `force_min` and `force_max`.
pub fn analyze_fatigue(
    material: &Material,
    wire_dia: Length,
    mean_dia: Length,
    force_min: Force,
    force_max: Force,
) -> Result<FatigueResult> {
    if force_max.newtons() < force_min.newtons() {
        return Err(SpringError::InconsistentInputs(
            "max cycle force must be at least the min cycle force".into(),
        ));
    }
    let endurance = material
        .endurance
        .ok_or_else(|| SpringError::NoFatigueData(material.name.clone()))?;

    let c = spring_index(mean_dia, wire_dia);
    let kb = bergstrasser_factor(c);
    let fa = Force::from_newtons((force_max.newtons() - force_min.newtons()) / 2.0);
    let fm = Force::from_newtons((force_max.newtons() + force_min.newtons()) / 2.0);
    let tau_a = corrected_shear_stress(fa, mean_dia, wire_dia, kb);
    let tau_m = corrected_shear_stress(fm, mean_dia, wire_dia, kb);

    let sut = material.min_tensile_strength(wire_dia)?.pascals();
    let ssu = SHEAR_TO_TENSILE * sut;
    // Convert Zimmerli pulsating data to a fully-reversed endurance (Shigley Eq. 10-31).
    let sse = endurance.ssa.pascals() / (1.0 - endurance.ssm.pascals() / ssu);
    // Goodman factor of safety: 1/nf = tau_a/Sse + tau_m/Ssu.
    let nf = 1.0 / (tau_a.pascals() / sse + tau_m.pascals() / ssu);

    Ok(FatigueResult {
        alternating_stress: tau_a,
        mean_stress: tau_m,
        fully_reversed_endurance: Stress::from_pascals(sse),
        ultimate_shear: Stress::from_pascals(ssu),
        goodman_factor_of_safety: nf,
    })
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod fatigue;
pub use fatigue::{analyze_fatigue, FatigueResult};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore fatigue`
Expected: PASS (3 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/fatigue.rs springcore/src/lib.rs
git commit -m "feat(fatigue): modified-Goodman fatigue with Zimmerli endurance data

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Design persistence (TOML save/load)

**Files:**
- Create: `springcore/src/persistence.rs`
- Modify: `springcore/src/lib.rs`

**Interfaces:**
- Consumes: `EndType`, `EndFixity`, `MaterialSet`, all scenario types, `SpringDesign`,
  `Result`, `SpringError`.
- Produces:
  - `enum UnitSystem { Us, Metric }` (serde)
  - `enum ScenarioSpec { PowerUser{..}, TwoLoad{..}, RateBased{..}, Dimensional{..} }`
    (serde, internally tagged `type`; lengths in mm, forces in N, rate in N/m,
    `end_type`/`fixity` as strings)
  - `struct SavedDesign { material: String, unit_system: UnitSystem, scenario: ScenarioSpec }`
    deriving `Serialize, Deserialize, PartialEq, Debug, Clone`, with:
    - `to_toml(&self) -> Result<String>`, `from_toml(&str) -> Result<Self>`
    - `save(&self, path: &std::path::Path) -> Result<()>`, `load(path: &std::path::Path) -> Result<Self>`
    - `solve(&self, materials: &MaterialSet) -> Result<SpringDesign>`

- [ ] **Step 1: Write the failing test**

In `springcore/src/persistence.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialSet;
    use approx::assert_relative_eq;

    fn sample() -> SavedDesign {
        SavedDesign {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            scenario: ScenarioSpec::RateBased {
                end_type: "squared_ground".into(),
                fixity: "fixed_fixed".into(),
                wire_dia_mm: 2.0,
                mean_dia_mm: 20.0,
                rate_n_per_m: 2000.0,
                free_length_mm: 60.0,
                loads_n: vec![10.0],
            },
        }
    }

    #[test]
    fn toml_roundtrip_is_lossless() {
        let original = sample();
        let text = original.to_toml().unwrap();
        let parsed = SavedDesign::from_toml(&text).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn solve_reproduces_design() {
        let set = MaterialSet::load_default();
        let design = sample().solve(&set).unwrap();
        assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    }

    #[test]
    fn unknown_end_type_is_rejected() {
        let mut s = sample();
        if let ScenarioSpec::RateBased { end_type, .. } = &mut s.scenario {
            *end_type = "banana".into();
        }
        assert!(s.solve(&MaterialSet::load_default()).is_err());
    }

    #[test]
    fn file_roundtrip() {
        let mut path = std::env::temp_dir();
        path.push("openspringmaker_test_design.toml");
        sample().save(&path).unwrap();
        let loaded = SavedDesign::load(&path).unwrap();
        assert_eq!(sample(), loaded);
        let _ = std::fs::remove_file(&path);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore persistence`
Expected: FAIL — types not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `springcore/src/persistence.rs`:

```rust
//! Human-readable persistence of a single design. Stores the user's inputs
//! (not computed outputs); the design is recomputed on load.

use crate::design::SpringDesign;
use crate::end_type::EndType;
use crate::material::MaterialSet;
use crate::mechanics::EndFixity;
use crate::scenario::{Dimensional, PowerUser, RateBased, Scenario, TwoLoad};
use crate::units::{Force, Length, SpringRate};
use crate::{Result, SpringError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Display unit system chosen for a saved design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitSystem {
    Us,
    Metric,
}

/// Serializable scenario inputs (SI-friendly primitives; lengths in mm).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ScenarioSpec {
    PowerUser {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
    TwoLoad {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        force1_n: f64,
        length1_mm: f64,
        force2_n: f64,
        length2_mm: f64,
    },
    RateBased {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        mean_dia_mm: f64,
        rate_n_per_m: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
    Dimensional {
        end_type: String,
        fixity: String,
        wire_dia_mm: f64,
        outer_dia_mm: f64,
        active: f64,
        free_length_mm: f64,
        loads_n: Vec<f64>,
    },
}

/// A persisted design: material, display units, and scenario inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDesign {
    pub material: String,
    pub unit_system: UnitSystem,
    pub scenario: ScenarioSpec,
}

fn parse_end_type(s: &str) -> Result<EndType> {
    Ok(match s {
        "plain" => EndType::Plain,
        "plain_ground" => EndType::PlainGround,
        "squared" => EndType::Squared,
        "squared_ground" => EndType::SquaredGround,
        other => return Err(SpringError::DataFile(format!("unknown end_type: {other}"))),
    })
}

fn parse_fixity(s: &str) -> Result<EndFixity> {
    Ok(match s {
        "fixed_fixed" => EndFixity::FixedFixed,
        "fixed_pinned" => EndFixity::FixedPinned,
        "pinned_pinned" => EndFixity::PinnedPinned,
        "fixed_free" => EndFixity::FixedFree,
        other => return Err(SpringError::DataFile(format!("unknown fixity: {other}"))),
    })
}

fn forces(v: &[f64]) -> Vec<Force> {
    v.iter().map(|&n| Force::from_newtons(n)).collect()
}

impl SavedDesign {
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_toml()?).map_err(|e| SpringError::DataFile(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| SpringError::DataFile(e.to_string()))?;
        Self::from_toml(&text)
    }

    pub fn solve(&self, materials: &MaterialSet) -> Result<SpringDesign> {
        let material = materials.get(&self.material)?;
        match &self.scenario {
            ScenarioSpec::PowerUser { end_type, fixity, wire_dia_mm, mean_dia_mm, active, free_length_mm, loads_n } => {
                PowerUser {
                    end_type: parse_end_type(end_type)?,
                    fixity: parse_fixity(fixity)?,
                    wire_dia: Length::from_millimeters(*wire_dia_mm),
                    mean_dia: Length::from_millimeters(*mean_dia_mm),
                    active: *active,
                    free_length: Length::from_millimeters(*free_length_mm),
                    loads: forces(loads_n),
                }
                .solve(material)
            }
            ScenarioSpec::TwoLoad { end_type, fixity, wire_dia_mm, mean_dia_mm, force1_n, length1_mm, force2_n, length2_mm } => {
                TwoLoad {
                    end_type: parse_end_type(end_type)?,
                    fixity: parse_fixity(fixity)?,
                    wire_dia: Length::from_millimeters(*wire_dia_mm),
                    mean_dia: Length::from_millimeters(*mean_dia_mm),
                    point1: (Force::from_newtons(*force1_n), Length::from_millimeters(*length1_mm)),
                    point2: (Force::from_newtons(*force2_n), Length::from_millimeters(*length2_mm)),
                }
                .solve(material)
            }
            ScenarioSpec::RateBased { end_type, fixity, wire_dia_mm, mean_dia_mm, rate_n_per_m, free_length_mm, loads_n } => {
                RateBased {
                    end_type: parse_end_type(end_type)?,
                    fixity: parse_fixity(fixity)?,
                    wire_dia: Length::from_millimeters(*wire_dia_mm),
                    mean_dia: Length::from_millimeters(*mean_dia_mm),
                    rate: SpringRate::from_newtons_per_meter(*rate_n_per_m),
                    free_length: Length::from_millimeters(*free_length_mm),
                    loads: forces(loads_n),
                }
                .solve(material)
            }
            ScenarioSpec::Dimensional { end_type, fixity, wire_dia_mm, outer_dia_mm, active, free_length_mm, loads_n } => {
                Dimensional {
                    end_type: parse_end_type(end_type)?,
                    fixity: parse_fixity(fixity)?,
                    wire_dia: Length::from_millimeters(*wire_dia_mm),
                    outer_dia: Length::from_millimeters(*outer_dia_mm),
                    active: *active,
                    free_length: Length::from_millimeters(*free_length_mm),
                    loads: forces(loads_n),
                }
                .solve(material)
            }
        }
    }
}
```

- [ ] **Step 4: Wire into the crate root**

In `springcore/src/lib.rs` add:

```rust
pub mod persistence;
pub use persistence::{SavedDesign, ScenarioSpec, UnitSystem};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore persistence`
Expected: PASS (4 tests).

Run: `cargo clippy -p springcore --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springcore/src/persistence.rs springcore/src/lib.rs
git commit -m "feat(persistence): TOML save/load of a design

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 13: Golden fixtures (accuracy contract)

**Files:**
- Create: `springcore/tests/golden.rs` (integration test crate)

**Interfaces:**
- Consumes: the full `springcore` public API.

This task has two parts. Part A is a self-contained end-to-end pipeline test that is
runnable immediately. Part B adds cross-checks against published worked examples; its
numeric literals are **transcribed from the physical reference at execution time** (the
same way a crypto test imports published test vectors). **Do not alter a published
expected value to make a disagreeing engine pass** — a disagreement beyond tolerance is
a real finding to root-cause (formula, material constant, or unit handling).

### Part A — end-to-end pipeline

- [ ] **Step 1: Write the pipeline test**

In `springcore/tests/golden.rs`:

```rust
//! Accuracy contract. Part A is self-contained; Part B cross-checks published
//! worked examples (values transcribed from the cited sources).

use approx::assert_relative_eq;
use springcore::{
    analyze_fatigue, evaluate_status, MaterialSet, SavedDesign, ScenarioSpec, UnitSystem,
};
use springcore::units::{Force, Length};

#[test]
fn pipeline_rate_based_music_wire() {
    // Clean reference case validated unit-by-unit in the module tests:
    // d=2mm, D=20mm (C=10), G=80 GPa, target rate 2000 N/m -> Na=10.
    let saved = SavedDesign {
        material: "Music Wire".into(),
        unit_system: UnitSystem::Metric,
        scenario: ScenarioSpec::RateBased {
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia_mm: 2.0,
            mean_dia_mm: 20.0,
            rate_n_per_m: 2000.0,
            free_length_mm: 60.0,
            loads_n: vec![10.0, 30.0],
        },
    };
    let set = MaterialSet::load_default();
    let design = saved.solve(&set).unwrap();

    assert_relative_eq!(design.index, 10.0, max_relative = 1e-9);
    assert_relative_eq!(design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
    assert_relative_eq!(design.active_coils, 10.0, max_relative = 1e-6);
    assert_relative_eq!(design.total_coils, 12.0, max_relative = 1e-9);
    assert_relative_eq!(design.solid_length.millimeters(), 24.0, max_relative = 1e-6);
    // 30 N -> 15 mm deflection at 2000 N/m.
    assert_relative_eq!(design.load_points[1].deflection.millimeters(), 15.0, max_relative = 1e-6);

    // Status: index 10 is in-range, so no index caution.
    let status = evaluate_status(&design, set.get("Music Wire").unwrap());
    assert!(!status.messages.iter().any(|m| m.message.contains("index")));

    // Fatigue over the 10–30 N cycle returns a finite, positive safety factor.
    let fat = analyze_fatigue(
        set.get("Music Wire").unwrap(),
        Length::from_millimeters(2.0),
        Length::from_millimeters(20.0),
        Force::from_newtons(10.0),
        Force::from_newtons(30.0),
    )
    .unwrap();
    assert!(fat.goodman_factor_of_safety > 1.0);
}
```

- [ ] **Step 2: Run Part A**

Run: `cargo test -p springcore --test golden pipeline_rate_based_music_wire`
Expected: PASS.

- [ ] **Step 3: Commit Part A**

```bash
git add springcore/tests/golden.rs
git commit -m "test(golden): end-to-end pipeline accuracy fixture

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Part B — published worked-example cross-checks

- [ ] **Step 4: Add the Shigley Example 10-1 cross-check**

Open Shigley's *Mechanical Engineering Design* (10th ed.), Example 10-1 (helical
compression spring of music wire). Record the example's **given inputs** (wire diameter,
outer or mean diameter, end type, number of coils or rate) and its **published results**
(spring rate, solid length, and the corrected stress at the stated load). Enter them in
this test. Construct the matching scenario, solve, and assert against the published
numbers with `max_relative = 0.03` (3% absorbs the source's rounding and any small
material-constant differences). If the example uses US units, build inputs with
`Length::from_inches` / `Force::from_pounds_force` and compare in those units.

```rust
#[test]
fn shigley_example_10_1() {
    // Inputs and expected values transcribed from Shigley 10th ed., Example 10-1.
    // Replace the values below with the figures printed in the source.
    let set = MaterialSet::load_default();
    let material = set.get("Music Wire").unwrap();

    // GIVEN (from the source):
    let wire_dia = Length::from_inches(/* d from Ex 10-1 */ 0.0);
    let mean_dia = Length::from_inches(/* D from Ex 10-1 */ 0.0);
    let active = /* Na from Ex 10-1 */ 0.0;
    let free_length = Length::from_inches(/* L0 from Ex 10-1 */ 0.0);
    let load = Force::from_pounds_force(/* F from Ex 10-1 */ 0.0);

    let design = springcore::PowerUser {
        end_type: /* end type from Ex 10-1 */ springcore::EndType::SquaredGround,
        fixity: springcore::EndFixity::FixedFixed,
        wire_dia,
        mean_dia,
        active,
        free_length,
        loads: vec![load],
    }
    .solve(material)
    .unwrap();

    // PUBLISHED RESULTS (from the source):
    assert_relative_eq!(design.rate.pounds_per_inch(), /* k */ 0.0, max_relative = 0.03);
    assert_relative_eq!(design.solid_length.inches(), /* Ls */ 0.0, max_relative = 0.03);
    assert_relative_eq!(design.load_points[0].shear_stress.psi(), /* tau */ 0.0, max_relative = 0.03);
}
```

This test starts `#[ignore]`-free but with zeroed inputs it will fail; the executor MUST
fill the source values before the task is considered done. (If the book is unavailable
during execution, mark this single test `#[ignore = "awaiting Shigley Ex 10-1 source values"]`
and record the gap in the PR body — do not delete it.)

- [ ] **Step 5: Add the EN 13906-1 cross-check (same pattern)**

Repeat Step 4 using the worked example in EN 13906-1 (metric: `from_millimeters`,
`from_newtons`, compare in SI). Name the test `en_13906_1_worked_example`. Same 3%
tolerance and the same no-fudging rule.

- [ ] **Step 6: Run Part B**

Run: `cargo test -p springcore --test golden`
Expected: PASS once source values are entered (or the two source tests `#[ignore]`d with
the gap recorded). Investigate any disagreement beyond 3% as a real engine bug.

- [ ] **Step 7: Commit Part B**

```bash
git add springcore/tests/golden.rs
git commit -m "test(golden): published worked-example cross-checks (Shigley, EN 13906-1)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 14: GUI form logic (pure, tested)

**Files:**
- Create: `springmaker/src/form.rs`
- Modify: `springmaker/src/main.rs` (add `mod form;` — keep the stub `main` for now)

**Interfaces:**
- Consumes: `springcore` public API.
- Produces (no iced dependency — fully unit-testable):
  - `enum ScenarioKind { PowerUser, TwoLoad, RateBased, Dimensional }`
  - `struct FormState { ... all input fields as String, plus material, unit_system, scenario, end_type, fixity }`
    with `Default`
  - `struct FormOutcome { design: SpringDesign, status: DesignStatus, fatigue: Option<FatigueResult> }`
  - `parse_and_solve(form: &FormState, materials: &MaterialSet) -> Result<FormOutcome>`

- [ ] **Step 1: Write the failing test**

In `springmaker/src/form.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use springcore::MaterialSet;
    use approx::assert_relative_eq;

    fn rate_based_metric() -> FormState {
        FormState {
            material: "Music Wire".into(),
            unit_system: springcore::UnitSystem::Metric,
            scenario: ScenarioKind::RateBased,
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: "2.0".into(),
            mean_dia: "20.0".into(),
            rate: "2000.0".into(),
            free_length: "60.0".into(),
            loads: "10, 30".into(),
            fatigue_min: "10".into(),
            fatigue_max: "30".into(),
            ..Default::default()
        }
    }

    #[test]
    fn solves_rate_based_metric() {
        let set = MaterialSet::load_default();
        let out = parse_and_solve(&rate_based_metric(), &set).unwrap();
        assert_relative_eq!(out.design.rate.newtons_per_meter(), 2000.0, max_relative = 1e-6);
        assert_eq!(out.design.load_points.len(), 2);
        assert!(out.fatigue.is_some());
    }

    #[test]
    fn us_units_are_converted() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.unit_system = springcore::UnitSystem::Us;
        form.wire_dia = "0.08".into(); // inches
        form.mean_dia = "0.8".into();
        form.rate = "10".into(); // lbf/in
        form.free_length = "2.0".into();
        form.loads = "2".into();
        form.fatigue_min = "1".into();
        form.fatigue_max = "2".into();
        let out = parse_and_solve(&form, &set).unwrap();
        assert_relative_eq!(out.design.wire_dia.inches(), 0.08, max_relative = 1e-9);
    }

    #[test]
    fn bad_number_is_an_error() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.wire_dia = "abc".into();
        assert!(parse_and_solve(&form, &set).is_err());
    }

    #[test]
    fn fatigue_absent_for_material_without_endurance() {
        let set = MaterialSet::load_default();
        let mut form = rate_based_metric();
        form.material = "Stainless 302".into();
        let out = parse_and_solve(&form, &set).unwrap();
        assert!(out.fatigue.is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springmaker form`
Expected: FAIL — types not found. (Add `mod form;` to `main.rs` first so the crate sees it.)

- [ ] **Step 3: Write minimal implementation**

At the top of `springmaker/src/form.rs`:

```rust
//! Pure form-to-design logic. No iced dependency, so it is unit-testable.

use springcore::units::{Force, Length, SpringRate};
use springcore::{
    analyze_fatigue, evaluate_status, DesignStatus, FatigueResult, MaterialSet, Result, SavedDesign,
    ScenarioSpec, SpringDesign, SpringError, UnitSystem,
};

/// Which scenario the form is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScenarioKind {
    #[default]
    PowerUser,
    TwoLoad,
    RateBased,
    Dimensional,
}

/// All editable form fields (as raw strings, mirroring the UI).
#[derive(Debug, Clone)]
pub struct FormState {
    pub material: String,
    pub unit_system: UnitSystem,
    pub scenario: ScenarioKind,
    pub end_type: String,
    pub fixity: String,
    pub wire_dia: String,
    pub mean_dia: String,
    pub outer_dia: String,
    pub active: String,
    pub free_length: String,
    pub rate: String,
    pub loads: String,
    pub force1: String,
    pub length1: String,
    pub force2: String,
    pub length2: String,
    pub fatigue_min: String,
    pub fatigue_max: String,
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            material: "Music Wire".into(),
            unit_system: UnitSystem::Metric,
            scenario: ScenarioKind::default(),
            end_type: "squared_ground".into(),
            fixity: "fixed_fixed".into(),
            wire_dia: String::new(),
            mean_dia: String::new(),
            outer_dia: String::new(),
            active: String::new(),
            free_length: String::new(),
            rate: String::new(),
            loads: String::new(),
            force1: String::new(),
            length1: String::new(),
            force2: String::new(),
            length2: String::new(),
            fatigue_min: String::new(),
            fatigue_max: String::new(),
        }
    }
}

/// A solved form: the design plus its status and optional fatigue result.
#[derive(Debug, Clone)]
pub struct FormOutcome {
    pub design: SpringDesign,
    pub status: DesignStatus,
    pub fatigue: Option<FatigueResult>,
}

fn num(field: &str, value: &str) -> Result<f64> {
    value
        .trim()
        .parse::<f64>()
        .map_err(|_| SpringError::InconsistentInputs(format!("{field} is not a number: '{value}'")))
}

fn length_mm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => Length::from_inches(v).millimeters(),
        UnitSystem::Metric => Length::from_millimeters(v).millimeters(),
    })
}

fn force_n(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => Force::from_pounds_force(v).newtons(),
        UnitSystem::Metric => Force::from_newtons(v).newtons(),
    })
}

fn rate_npm(field: &str, value: &str, us: UnitSystem) -> Result<f64> {
    let v = num(field, value)?;
    Ok(match us {
        UnitSystem::Us => SpringRate::from_pounds_per_inch(v).newtons_per_meter(),
        UnitSystem::Metric => SpringRate::from_newtons_per_meter(v).newtons_per_meter(),
    })
}

fn loads_n(value: &str, us: UnitSystem) -> Result<Vec<f64>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| force_n("load", s, us))
        .collect()
}

fn build_spec(form: &FormState) -> Result<ScenarioSpec> {
    let us = form.unit_system;
    Ok(match form.scenario {
        ScenarioKind::PowerUser => ScenarioSpec::PowerUser {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            active: num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
        ScenarioKind::TwoLoad => ScenarioSpec::TwoLoad {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            force1_n: force_n("force 1", &form.force1, us)?,
            length1_mm: length_mm("length 1", &form.length1, us)?,
            force2_n: force_n("force 2", &form.force2, us)?,
            length2_mm: length_mm("length 2", &form.length2, us)?,
        },
        ScenarioKind::RateBased => ScenarioSpec::RateBased {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            mean_dia_mm: length_mm("mean diameter", &form.mean_dia, us)?,
            rate_n_per_m: rate_npm("rate", &form.rate, us)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
        ScenarioKind::Dimensional => ScenarioSpec::Dimensional {
            end_type: form.end_type.clone(),
            fixity: form.fixity.clone(),
            wire_dia_mm: length_mm("wire diameter", &form.wire_dia, us)?,
            outer_dia_mm: length_mm("outer diameter", &form.outer_dia, us)?,
            active: num("active coils", &form.active)?,
            free_length_mm: length_mm("free length", &form.free_length, us)?,
            loads_n: loads_n(&form.loads, us)?,
        },
    })
}

/// Parse the form, solve the design, evaluate status, and (if a cycle and endurance
/// data are present) compute fatigue. Missing endurance data degrades to `None`.
pub fn parse_and_solve(form: &FormState, materials: &MaterialSet) -> Result<FormOutcome> {
    let saved = SavedDesign {
        material: form.material.clone(),
        unit_system: form.unit_system,
        scenario: build_spec(form)?,
    };
    let design = saved.solve(materials)?;
    let material = materials.get(&form.material)?;
    let status = evaluate_status(&design, material);

    let fatigue = if form.fatigue_min.trim().is_empty() || form.fatigue_max.trim().is_empty() {
        None
    } else {
        let fmin = Force::from_newtons(force_n("fatigue min", &form.fatigue_min, form.unit_system)?);
        let fmax = Force::from_newtons(force_n("fatigue max", &form.fatigue_max, form.unit_system)?);
        match analyze_fatigue(material, design.wire_dia, design.mean_dia, fmin, fmax) {
            Ok(r) => Some(r),
            Err(SpringError::NoFatigueData(_)) => None,
            Err(e) => return Err(e),
        }
    };

    Ok(FormOutcome { design, status, fatigue })
}
```

- [ ] **Step 4: Add `units` re-export access**

`parse_and_solve` uses `springcore::units::*`. Ensure `springcore/src/lib.rs` keeps
`pub mod units;` (done in Task 2) so `springcore::units::Length` is reachable.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springmaker form`
Expected: PASS (4 tests).

Run: `cargo clippy -p springmaker --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add springmaker/src/form.rs springmaker/src/main.rs
git commit -m "feat(gui): pure, tested form-to-design logic

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 15: iced application wiring

**Files:**
- Create: `springmaker/src/app.rs`, `springmaker/src/view.rs`
- Modify: `springmaker/src/main.rs` (real entry point), `springmaker/Cargo.toml` (add `rfd`)

**Interfaces:**
- Consumes: `form` module, `springcore`, `iced`, `rfd`.
- Produces: `struct App`, `enum Message`, `enum Field`, `App::update`, `App::view`,
  `App::recompute`, and a `main` that runs the iced application.

- [ ] **Step 1: Verify the iced 0.13 and rfd APIs before coding**

Use context7 to fetch current docs (do not rely on memory — iced's API changes between
releases):
- Resolve and query the `iced` crate (v0.13): confirm the application entry
  (`iced::application(title, update, view).run()` requiring `App: Default`), the
  `Element`/`Task` types, and the `text_input`, `pick_list`, `column`, `row`, `container`,
  `scrollable`, `button`, `radio`/`toggler` widget signatures.
- Resolve and query `rfd` (v0.14): confirm `rfd::FileDialog::new().save_file()` /
  `.pick_file()` blocking calls return `Option<PathBuf>`.

Adjust the code below to match the confirmed signatures. The structure (state, message,
update, view, recompute) stays the same regardless of minor signature differences.

- [ ] **Step 2: Add the file-dialog dependency**

In `springmaker/Cargo.toml` add under `[dependencies]`:

```toml
rfd = "0.14"
```

- [ ] **Step 3: Write the failing test (pure recompute logic)**

In `springmaker/src/app.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_has_no_outcome_until_filled() {
        let app = App::default();
        assert!(app.outcome.is_none());
        assert_eq!(app.form.material, "Music Wire");
    }

    #[test]
    fn recompute_produces_outcome_for_valid_form() {
        let mut app = App::default();
        app.form.scenario = crate::form::ScenarioKind::RateBased;
        app.form.wire_dia = "2.0".into();
        app.form.mean_dia = "20.0".into();
        app.form.rate = "2000".into();
        app.form.free_length = "60".into();
        app.form.loads = "10, 30".into();
        app.recompute();
        assert!(app.error.is_none());
        assert!(app.outcome.is_some());
    }

    #[test]
    fn recompute_sets_error_for_invalid_form() {
        let mut app = App::default();
        app.form.scenario = crate::form::ScenarioKind::RateBased;
        app.form.wire_dia = "oops".into();
        app.recompute();
        assert!(app.outcome.is_none());
        assert!(app.error.is_some());
    }
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test -p springmaker app`
Expected: FAIL — `App` not found.

- [ ] **Step 5: Write the implementation**

`springmaker/src/app.rs` (above the test module). Adjust widget/entry calls to the
context7-confirmed iced 0.13 API:

```rust
//! Application state, messages, and update/view glue for the iced GUI.

use crate::form::{parse_and_solve, FormOutcome, FormState, ScenarioKind};
use crate::view;
use springcore::{MaterialSet, SavedDesign, ScenarioSpec, UnitSystem};

/// Which text field a `Message::Field` targets.
#[derive(Debug, Clone, Copy)]
pub enum Field {
    WireDia,
    MeanDia,
    OuterDia,
    Active,
    FreeLength,
    Rate,
    Loads,
    Force1,
    Length1,
    Force2,
    Length2,
    FatigueMin,
    FatigueMax,
}

/// All UI events.
#[derive(Debug, Clone)]
pub enum Message {
    Field(Field, String),
    Material(String),
    Scenario(ScenarioKind),
    Units(UnitSystem),
    EndType(String),
    Fixity(String),
    Save,
    Load,
}

/// Top-level application state.
pub struct App {
    pub form: FormState,
    pub materials: MaterialSet,
    pub outcome: Option<FormOutcome>,
    pub error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            form: FormState::default(),
            materials: MaterialSet::load_default(),
            outcome: None,
            error: None,
        }
    }
}

impl App {
    /// Re-solve from the current form, storing either an outcome or an error string.
    pub fn recompute(&mut self) {
        match parse_and_solve(&self.form, &self.materials) {
            Ok(out) => {
                self.outcome = Some(out);
                self.error = None;
            }
            Err(e) => {
                self.outcome = None;
                self.error = Some(e.to_string());
            }
        }
    }

    pub fn title(&self) -> String {
        "OpenSpringmaker".into()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Field(field, value) => self.set_field(field, value),
            Message::Material(m) => self.form.material = m,
            Message::Scenario(s) => self.form.scenario = s,
            Message::Units(u) => self.form.unit_system = u,
            Message::EndType(e) => self.form.end_type = e,
            Message::Fixity(f) => self.form.fixity = f,
            Message::Save => self.save_dialog(),
            Message::Load => self.load_dialog(),
        }
        self.recompute();
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        view::view(self)
    }

    fn set_field(&mut self, field: Field, value: String) {
        let f = &mut self.form;
        match field {
            Field::WireDia => f.wire_dia = value,
            Field::MeanDia => f.mean_dia = value,
            Field::OuterDia => f.outer_dia = value,
            Field::Active => f.active = value,
            Field::FreeLength => f.free_length = value,
            Field::Rate => f.rate = value,
            Field::Loads => f.loads = value,
            Field::Force1 => f.force1 = value,
            Field::Length1 => f.length1 = value,
            Field::Force2 => f.force2 = value,
            Field::Length2 => f.length2 = value,
            Field::FatigueMin => f.fatigue_min = value,
            Field::FatigueMax => f.fatigue_max = value,
        }
    }

    fn save_dialog(&mut self) {
        let spec = match crate::form::build_spec_public(&self.form) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if let Some(path) = rfd::FileDialog::new().add_filter("design", &["toml"]).save_file() {
            let saved = SavedDesign {
                material: self.form.material.clone(),
                unit_system: self.form.unit_system,
                scenario: spec,
            };
            if let Err(e) = saved.save(&path) {
                self.error = Some(e.to_string());
            }
        }
    }

    fn load_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("design", &["toml"]).pick_file() {
            match SavedDesign::load(&path) {
                Ok(saved) => self.apply_saved(saved),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
    }

    fn apply_saved(&mut self, saved: SavedDesign) {
        // Populate the form from a loaded design (display in its saved unit system).
        self.form.material = saved.material;
        self.form.unit_system = saved.unit_system;
        crate::form::populate_from_spec(&mut self.form, &saved.scenario);
    }
}
```

This references two helpers to add to `form.rs`: `build_spec_public` (a `pub` wrapper
around the existing `build_spec`) and `populate_from_spec(form: &mut FormState, spec: &ScenarioSpec)`
which writes the spec's fields back into the form strings (converting mm→display units
per `form.unit_system`). Add both to `form.rs` with a unit test for the round-trip
(`FormState` → `build_spec_public` → `populate_from_spec` → equal field strings for a
metric form).

- [ ] **Step 6: Write the view module**

`springmaker/src/view.rs` — build the layout with the context7-confirmed widget API.
Required structure (a function `pub fn view(app: &App) -> iced::Element<'_, Message>`):
- A header row with the material `pick_list` (options = `app.materials.names()`), the
  scenario `pick_list`, a US/metric unit toggle (`radio` or `toggler`), and the end-type
  and fixity `pick_list`s.
- An input column whose visible fields depend on `app.form.scenario`: fields not owned by
  the active scenario are not rendered (or rendered disabled/dimmed) — never silently
  accept edits to inactive fields. Each `text_input` emits `Message::Field(Field::X, _)`.
- An output column reading `app.outcome`: index, rate (in display units), active/total
  coils, solid/free length, OD/ID, natural frequency, buckling-stable flag, and a per-load
  table (force, deflection, length, stress, % of MTS). Show fatigue results when present,
  or the text "No fatigue data for this material" when `outcome.fatigue` is `None`.
- A **Design Status** panel listing `app.outcome.status.messages`, colored by severity,
  and `app.error` (if any) shown prominently.
- Save / Load buttons emitting `Message::Save` / `Message::Load`.
- Wrap everything in a `scrollable` `container`.

Keep `view.rs` focused on layout only; all computation lives in `form`/`springcore`.

- [ ] **Step 7: Write the real `main`**

Replace `springmaker/src/main.rs` with:

```rust
mod app;
mod form;
mod plot; // added in Task 16; create an empty `pub fn noop() {}` placeholder now if needed
mod view;

use app::App;

fn main() -> iced::Result {
    iced::application(App::title, App::update, App::view).run()
}
```

(If `iced::application` in 0.13 takes the title as a `&str` or a closure differently than
shown, use the context7-confirmed form. `run()` constructs the state via `App::default()`.)

- [ ] **Step 8: Run tests and the app**

Run: `cargo test -p springmaker` → the `app` and `form` tests PASS.
Run: `cargo clippy -p springmaker --all-targets -- -D warnings` → clean.
Run: `cargo run -p springmaker` → the window opens; enter a Rate-Based metric design
(d=2, D=20, rate=2000, free length=60, loads "10, 30") and confirm live outputs appear.
Capture a screenshot for the PR.

- [ ] **Step 9: Commit**

```bash
git add springmaker/ 
git commit -m "feat(gui): iced application with live solve, status panel, save/load

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 16: Live results plot (plotters-iced)

**Files:**
- Create: `springmaker/src/plot.rs` (replacing the Task 15 placeholder)
- Modify: `springmaker/src/view.rs` (embed the chart)

**Interfaces:**
- Consumes: `springcore::SpringDesign`, `springcore::UnitSystem`, `plotters`, `plotters-iced`, `iced`.
- Produces:
  - `force_deflection_series(design: &SpringDesign, units: UnitSystem) -> Vec<(f64, f64)>`
    (deflection on x, force on y, in display units) — pure, tested.
  - `struct ResultsChart<'a> { design: &'a SpringDesign, units: UnitSystem }` implementing
    `plotters_iced::Chart<Message>`.
  - `pub fn results_chart(design: &SpringDesign, units: UnitSystem) -> iced::Element<'_, Message>`.

- [ ] **Step 1: Verify the plotters-iced 0.11 API via context7**

Query context7 for `plotters-iced` (v0.11): confirm the `Chart` trait associated types
and method (`type State`, `fn build_chart(&self, state: &Self::State, builder: ChartBuilder<...>)`),
and `ChartWidget::new(...)`. Confirm it targets `iced` 0.13. Adjust the code to match.

- [ ] **Step 2: Write the failing test (pure series)**

In `springmaker/src/plot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use springcore::units::{Force, Length};
    use springcore::{MaterialSet, PowerUser, Scenario, UnitSystem};
    use springcore::mechanics::EndFixity;
    use springcore::EndType;
    use approx::assert_relative_eq;

    fn design() -> springcore::SpringDesign {
        let m = MaterialSet::load_default().get("Music Wire").unwrap().clone();
        PowerUser {
            end_type: EndType::SquaredGround,
            fixity: EndFixity::FixedFixed,
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            active: 10.0,
            free_length: Length::from_millimeters(60.0),
            loads: vec![Force::from_newtons(10.0), Force::from_newtons(30.0)],
        }
        .solve(&m)
        .unwrap()
    }

    #[test]
    fn series_starts_at_origin_and_is_linear() {
        let s = force_deflection_series(&design(), UnitSystem::Metric);
        assert!(s.len() >= 2);
        assert_relative_eq!(s[0].0, 0.0, max_relative = 1e-12);
        assert_relative_eq!(s[0].1, 0.0, max_relative = 1e-12);
        // Last point: at 30 N, deflection 15 mm (k=2000 N/m). x in mm, y in N.
        let last = s.last().unwrap();
        assert_relative_eq!(last.0, 15.0, max_relative = 1e-6);
        assert_relative_eq!(last.1, 30.0, max_relative = 1e-6);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p springmaker plot`
Expected: FAIL — `force_deflection_series` not found.

- [ ] **Step 4: Write the implementation**

`springmaker/src/plot.rs` (above the test module). Adjust the `Chart` impl to the
context7-confirmed plotters-iced 0.11 API:

```rust
//! Live load-vs-deflection chart for the current design.

use crate::app::Message;
use plotters::prelude::*;
use plotters_iced::{Chart, ChartWidget};
use springcore::{SpringDesign, UnitSystem};

/// Force–deflection points (deflection x, force y) in the display unit system.
/// The line is linear (rate is constant), so endpoints suffice; the largest
/// operating load sets the extent.
pub fn force_deflection_series(design: &SpringDesign, units: UnitSystem) -> Vec<(f64, f64)> {
    let max_force = design
        .load_points
        .iter()
        .map(|lp| lp.force.newtons())
        .fold(0.0_f64, f64::max)
        .max(design.at_solid.force.newtons());
    let rate = design.rate.newtons_per_meter();
    let max_defl_m = if rate > 0.0 { max_force / rate } else { 0.0 };

    let convert = |defl_m: f64, force_n: f64| match units {
        UnitSystem::Metric => (
            springcore::units::Length::from_meters(defl_m).millimeters(),
            springcore::units::Force::from_newtons(force_n).newtons(),
        ),
        UnitSystem::Us => (
            springcore::units::Length::from_meters(defl_m).inches(),
            springcore::units::Force::from_newtons(force_n).pounds_force(),
        ),
    };

    vec![convert(0.0, 0.0), convert(max_defl_m, max_force)]
}

/// Chart wrapper for the current design.
pub struct ResultsChart<'a> {
    pub design: &'a SpringDesign,
    pub units: UnitSystem,
}

impl<'a> Chart<Message> for ResultsChart<'a> {
    type State = ();

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, mut builder: ChartBuilder<DB>) {
        let series = force_deflection_series(self.design, self.units);
        let x_max = series.iter().map(|p| p.0).fold(0.0_f64, f64::max).max(1e-9);
        let y_max = series.iter().map(|p| p.1).fold(0.0_f64, f64::max).max(1e-9);

        let mut chart = builder
            .margin(20)
            .x_label_area_size(30)
            .y_label_area_size(40)
            .build_cartesian_2d(0.0..x_max * 1.1, 0.0..y_max * 1.1)
            .expect("chart axes");

        chart.configure_mesh().x_desc("deflection").y_desc("load").draw().expect("mesh");
        chart
            .draw_series(LineSeries::new(series.iter().copied(), &BLUE))
            .expect("line");
        // Operating-point markers.
        let pts: Vec<(f64, f64)> = self
            .design
            .load_points
            .iter()
            .map(|lp| match self.units {
                UnitSystem::Metric => (lp.deflection.millimeters(), lp.force.newtons()),
                UnitSystem::Us => (lp.deflection.inches(), lp.force.pounds_force()),
            })
            .collect();
        chart
            .draw_series(pts.iter().map(|&(x, y)| Circle::new((x, y), 4, RED.filled())))
            .expect("markers");
    }
}

/// Build the chart widget element.
pub fn results_chart(design: &SpringDesign, units: UnitSystem) -> iced::Element<'_, Message> {
    ChartWidget::new(ResultsChart { design, units }).into()
}
```

- [ ] **Step 5: Embed the chart in the view**

In `springmaker/src/view.rs`, when `app.outcome` is `Some(out)`, add
`crate::plot::results_chart(&out.design, app.form.unit_system)` to the output column
(give it a fixed height via the iced `container`/`Length::Fixed` per the confirmed API).

- [ ] **Step 6: Run tests and the app**

Run: `cargo test -p springmaker plot` → PASS.
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean.
Run: `cargo run -p springmaker` → the chart renders and updates live as loads change.
Capture an updated screenshot for the PR.

- [ ] **Step 7: Commit**

```bash
git add springmaker/src/plot.rs springmaker/src/view.rs
git commit -m "feat(gui): live load-vs-deflection chart via plotters-iced

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final verification

- [ ] **Step 1: Full workspace gate**

Run: `cargo fmt --all -- --check` → no diff.
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean.
Run: `cargo test --workspace` → all tests pass (note any `#[ignore]`d golden source
tests and why in the PR body).
Run: `cargo build --workspace --release` → builds.

- [ ] **Step 2: Update docs and run the mandatory review panel**

Ensure `README.md`, `ARCHITECTURE.md`, and the ADRs reflect the final module layout
(`scenario.rs`, `optimize.rs`, `form.rs`, `app.rs`, `view.rs`, `plot.rs`). Then run the
mandatory adversarial multi-agent code review (general reviewer, architect, simplifier,
plus a wire-format/units specialist and a numerical-methods specialist for the solver),
cycle to convergence, and only then open the PR.

