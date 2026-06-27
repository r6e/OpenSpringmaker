# Helical Torsion Springs — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add helical torsion springs (round wire) as a third spring family in the `springcore` engine — new units, mechanics, a static solver, and a `PowerUser` scenario — all behind cited formulas with strict TDD.

**Architecture:** A new self-contained `springcore/src/torsion/` module (mechanics, design, scenario) parallel to `extension/`, plus three new shared `units.rs` quantities (Moment, Angle, AngularRate). Torsion is loaded by a moment `M`, deflects through an angle `θ`, and is stressed in bending; the angular rate is selectable between Shigley's friction model and EN 13906-3 pure bending.

**Tech Stack:** Rust (workspace crate `springcore`), `approx` for test asserts, `serde` (units derive it), `cargo test`/`clippy`/`mutants`.

## Global Constraints

- MSRV 1.88; dual MIT/Apache; SI canonical internally (angle in radians, angular rate in N·m/rad), convert at the boundary.
- Every formula cited inline: Shigley Ch. 10 torsion-spring equations (10-43 K_bi, 10-49 wound diameter, 10-50 active coils, 10-51 rate) and EN 13906-3 (pure-bending rate); design spec `docs/superpowers/specs/2026-06-26-torsion-springs-phase1-design.md`.
- No commercial-product/vendor references in any persisted file (technical citations such as Shigley and EN standards are permitted).
- Reuse `Material.allowable_pct_bending` (already exists) — NO new material field.
- `FrictionModel` is deliberately NOT `#[non_exhaustive]` (unpublished crate; the GUI will match it and wants exhaustiveness — per the PR #32 scope decision).
- Strict TDD; before any push: `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, repo-wide `typos`, `cargo deny check all`, and `cargo mutants --in-diff` (springcore, literal 0 survivors) all green.
- Branch `feat/torsion-springs-phase1` (already created off main; the design spec is committed there).
- Mandatory adversarial multi-agent review panel before every push, cycling to convergence.

## File Structure

- `springcore/src/units.rs` (MODIFY) — add `Moment`, `Angle`, `AngularRate` quantities + conversion impls.
- `springcore/src/lib.rs` (MODIFY) — register `pub mod torsion;` and re-export the new units + torsion public types.
- `springcore/src/torsion/mod.rs` (CREATE) — module doc + `mod` declarations + `pub use` re-exports.
- `springcore/src/torsion/mechanics.rs` (CREATE) — `FrictionModel` + pure formula functions.
- `springcore/src/torsion/design.rs` (CREATE) — input/result types, `solve_forward`, status checks.
- `springcore/src/torsion/scenario.rs` (CREATE) — `Scenario` trait + `PowerUser`.

## Reference patterns (read before starting)

- Units macro + conversion impls: `springcore/src/units.rs:17-145` (`si_quantity!`, `Length`/`Force`/`SpringRate` impls).
- Solver + validation style: `springcore/src/extension/design.rs:93-185` (`solve_forward` guard cascade returning `SpringError::InconsistentInputs`).
- Scenario trait + PowerUser: `springcore/src/extension/scenario.rs:1-60`.
- Status types (reuse, do not redefine): `springcore/src/design.rs:218-247` (`Severity{Info,Caution,Warning}`, `StatusMessage`, `DesignStatus`).
- Mechanics that take `Stress` moduli directly: `springcore/src/mechanics.rs:144-150`.
- Material accessors: `Material.youngs_modulus: Stress`, `Material.allowable_pct_bending: f64`, `Material.min_tensile_strength(d: Length) -> Result<Stress>` (`springcore/src/material.rs:226-251`).

---

### Task 1: New units — Moment, Angle, AngularRate

**Files:**
- Modify: `springcore/src/units.rs`
- Modify: `springcore/src/lib.rs:46` (re-export line)

**Interfaces:**
- Produces:
  - `Moment` — `from_newton_meters(f64)`, `from_newton_millimeters(f64)`, `from_pound_force_inches(f64)`, `newton_meters()`, `newton_millimeters()`, `pound_force_inches()`.
  - `Angle` — `from_radians(f64)`, `from_degrees(f64)`, `from_turns(f64)`, `radians()`, `degrees()`, `turns()`.
  - `AngularRate` — `from_newton_meters_per_radian(f64)`, `from_newton_meters_per_degree(f64)`, `from_newton_meters_per_turn(f64)`, `newton_meters_per_radian()`, `newton_meters_per_degree()`, `newton_meters_per_turn()`.

- [ ] **Step 1: Write failing unit round-trip tests**

In `springcore/src/units.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block, add:

```rust
#[test]
fn moment_conversions_round_trip() {
    let m = Moment::from_newton_meters(2.0);
    assert_relative_eq!(m.newton_meters(), 2.0, max_relative = 1e-12);
    assert_relative_eq!(m.newton_millimeters(), 2000.0, max_relative = 1e-12);
    // 1 lbf·in = 4.4482216152605 N × 0.0254 m = 0.112984829... N·m
    let one_lbf_in = Moment::from_pound_force_inches(1.0);
    assert_relative_eq!(one_lbf_in.newton_meters(), 0.1129848290276167, max_relative = 1e-12);
    assert_relative_eq!(one_lbf_in.pound_force_inches(), 1.0, max_relative = 1e-12);
}

#[test]
fn angle_conversions_round_trip() {
    use std::f64::consts::{PI, TAU};
    let a = Angle::from_degrees(180.0);
    assert_relative_eq!(a.radians(), PI, max_relative = 1e-12);
    assert_relative_eq!(a.turns(), 0.5, max_relative = 1e-12);
    let one_turn = Angle::from_turns(1.0);
    assert_relative_eq!(one_turn.radians(), TAU, max_relative = 1e-12);
    assert_relative_eq!(one_turn.degrees(), 360.0, max_relative = 1e-12);
}

#[test]
fn angular_rate_conversions_round_trip() {
    use std::f64::consts::{PI, TAU};
    // 1 N·m/rad → per degree = ×(π/180); per turn = ×2π.
    let k = AngularRate::from_newton_meters_per_radian(1.0);
    assert_relative_eq!(k.newton_meters_per_degree(), PI / 180.0, max_relative = 1e-12);
    assert_relative_eq!(k.newton_meters_per_turn(), TAU, max_relative = 1e-12);
    let per_turn = AngularRate::from_newton_meters_per_turn(TAU);
    assert_relative_eq!(per_turn.newton_meters_per_radian(), 1.0, max_relative = 1e-12);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p springcore --lib units::tests::moment_conversions_round_trip`
Expected: FAIL (compile error — `Moment` not found).

- [ ] **Step 3: Add the three quantities**

In `springcore/src/units.rs`, after the existing `si_quantity!(... Temperature)` block (around line 53), add:

```rust
si_quantity!(
    /// Bending/torsional moment (torque), stored in newton-metres.
    Moment
);
si_quantity!(
    /// Angle, stored in radians (SI).
    Angle
);
si_quantity!(
    /// Angular spring rate (moment per angle), stored in newton-metres per radian.
    AngularRate
);
```

- [ ] **Step 4: Add the conversion impls**

After the existing `impl SpringRate { ... }` block, add:

```rust
impl Moment {
    /// Construct from newton-metres (SI base unit).
    pub fn from_newton_meters(v: f64) -> Self {
        Self(v)
    }
    /// Construct from newton-millimetres (1 N·mm = 0.001 N·m).
    pub fn from_newton_millimeters(v: f64) -> Self {
        Self(v / 1000.0)
    }
    /// Construct from pound-force inches (1 lbf·in = 4.4482216152605 N × 0.0254 m).
    pub fn from_pound_force_inches(v: f64) -> Self {
        Self(v * NEWTONS_PER_LBF * METERS_PER_INCH)
    }
    /// Return value in newton-metres.
    pub fn newton_meters(self) -> f64 {
        self.0
    }
    /// Return value in newton-millimetres.
    pub fn newton_millimeters(self) -> f64 {
        self.0 * 1000.0
    }
    /// Return value in pound-force inches.
    pub fn pound_force_inches(self) -> f64 {
        self.0 / (NEWTONS_PER_LBF * METERS_PER_INCH)
    }
}

impl Angle {
    /// Construct from radians (SI base unit).
    pub fn from_radians(v: f64) -> Self {
        Self(v)
    }
    /// Construct from degrees (1 deg = π/180 rad).
    pub fn from_degrees(v: f64) -> Self {
        Self(v * std::f64::consts::PI / 180.0)
    }
    /// Construct from turns / revolutions (1 turn = 2π rad).
    pub fn from_turns(v: f64) -> Self {
        Self(v * std::f64::consts::TAU)
    }
    /// Return value in radians.
    pub fn radians(self) -> f64 {
        self.0
    }
    /// Return value in degrees.
    pub fn degrees(self) -> f64 {
        self.0 * 180.0 / std::f64::consts::PI
    }
    /// Return value in turns / revolutions.
    pub fn turns(self) -> f64 {
        self.0 / std::f64::consts::TAU
    }
}

impl AngularRate {
    /// Construct from newton-metres per radian (SI base unit).
    pub fn from_newton_meters_per_radian(v: f64) -> Self {
        Self(v)
    }
    /// Construct from newton-metres per degree (1 N·m/deg = 180/π N·m/rad).
    pub fn from_newton_meters_per_degree(v: f64) -> Self {
        Self(v * 180.0 / std::f64::consts::PI)
    }
    /// Construct from newton-metres per turn (1 N·m/turn = 1/2π N·m/rad).
    pub fn from_newton_meters_per_turn(v: f64) -> Self {
        Self(v / std::f64::consts::TAU)
    }
    /// Return value in newton-metres per radian.
    pub fn newton_meters_per_radian(self) -> f64 {
        self.0
    }
    /// Return value in newton-metres per degree.
    pub fn newton_meters_per_degree(self) -> f64 {
        self.0 * std::f64::consts::PI / 180.0
    }
    /// Return value in newton-metres per turn.
    pub fn newton_meters_per_turn(self) -> f64 {
        self.0 * std::f64::consts::TAU
    }
}
```

- [ ] **Step 5: Re-export from lib.rs**

In `springcore/src/lib.rs`, change the units re-export (line ~46) from:

```rust
pub use units::{Force, Frequency, Length, MassDensity, SpringRate, Stress, Temperature};
```
to:
```rust
pub use units::{
    Angle, AngularRate, Force, Frequency, Length, MassDensity, Moment, SpringRate, Stress,
    Temperature,
};
```

- [ ] **Step 6: Run tests and gates**

Run: `cargo test -p springcore --lib units:: && cargo clippy -p springcore --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/units.rs springcore/src/lib.rs
git commit -m "feat(units): add Moment, Angle, AngularRate (radian-canonical) for torsion springs"
```

---

### Task 2: Torsion mechanics — FrictionModel + formula functions

**Files:**
- Create: `springcore/src/torsion/mod.rs`
- Create: `springcore/src/torsion/mechanics.rs`
- Modify: `springcore/src/lib.rs` (register `pub mod torsion;` after `pub mod extension;`)

**Interfaces:**
- Consumes: `Moment`, `Angle`, `AngularRate`, `Length`, `Stress` (Task 1 / existing).
- Produces:
  - `pub enum FrictionModel { ShigleyFriction, PureBending }` with `Default` = `ShigleyFriction`.
  - `pub fn kbi_factor(c: f64) -> f64`
  - `pub fn bending_stress_inner(moment: Moment, mean_dia: Length, wire_dia: Length) -> Stress`
  - `pub fn bending_stress_nominal(moment: Moment, wire_dia: Length) -> Stress`
  - `pub fn active_coils_with_legs(body_coils: f64, leg1: Length, leg2: Length, mean_dia: Length) -> f64`
  - `pub fn angular_rate(youngs_modulus: Stress, wire_dia: Length, mean_dia: Length, active: f64, friction: FrictionModel) -> AngularRate`
  - `pub fn wound_mean_diameter(mean_dia: Length, body_coils: f64, deflection: Angle) -> Length`

- [ ] **Step 1: Register the module**

In `springcore/src/lib.rs`, add after `pub mod extension;`:

```rust
pub mod torsion;
```

Create `springcore/src/torsion/mod.rs`:

```rust
//! Helical torsion springs (round wire). Parallel to the compression and extension
//! families; loaded by a moment M, deflecting through an angle θ, stressed in bending.
//! Reuses `units`, `material`, `numeric`, and the crate-root `DesignStatus`. Formula
//! sources cited at each call site (Shigley Ch. 10; EN 13906-3).

mod mechanics;

pub use mechanics::{
    active_coils_with_legs, angular_rate, bending_stress_inner, bending_stress_nominal,
    kbi_factor, wound_mean_diameter, FrictionModel,
};
```

- [ ] **Step 2: Write failing mechanics tests**

Create `springcore/src/torsion/mechanics.rs` with ONLY a test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Angle, Length, Stress};
    use approx::assert_relative_eq;

    #[test]
    fn kbi_at_index_ten() {
        // K_bi = (4C²−C−1)/(4C(C−1)); C=10 → (400−10−1)/(360) = 389/360.
        assert_relative_eq!(kbi_factor(10.0), 389.0 / 360.0, max_relative = 1e-12);
    }

    #[test]
    fn nominal_bending_stress_value() {
        // σ₀ = 32M/(πd³); M=1 N·m, d=2 mm → 32/(π·8e-9) = 1.2732395447e9 Pa.
        let s = bending_stress_nominal(Moment::from_newton_meters(1.0), Length::from_millimeters(2.0));
        assert_relative_eq!(s.pascals(), 1.2732395447351628e9, max_relative = 1e-9);
    }

    #[test]
    fn inner_bending_stress_applies_kbi() {
        // σᵢ = K_bi·σ₀; C=10 → 389/360 × 1.2732395447e9 = 1.375803...e9 Pa.
        let si = bending_stress_inner(
            Moment::from_newton_meters(1.0),
            Length::from_millimeters(20.0),
            Length::from_millimeters(2.0),
        );
        assert_relative_eq!(si.pascals(), (389.0 / 360.0) * 1.2732395447351628e9, max_relative = 1e-9);
    }

    #[test]
    fn active_coils_adds_leg_term() {
        // Na = N_b + (L1+L2)/(3πD); N_b=5, L1=L2=50 mm, D=20 mm.
        // (0.05+0.05)/(3π·0.02) = 0.1/0.1884955592 = 0.5305164769.
        let na = active_coils_with_legs(
            5.0,
            Length::from_millimeters(50.0),
            Length::from_millimeters(50.0),
            Length::from_millimeters(20.0),
        );
        assert_relative_eq!(na, 5.530516476972984, max_relative = 1e-12);
    }

    #[test]
    fn active_coils_body_only_when_no_legs() {
        let na = active_coils_with_legs(5.0, Length::from_meters(0.0), Length::from_meters(0.0), Length::from_millimeters(20.0));
        assert_relative_eq!(na, 5.0, max_relative = 1e-12);
    }

    #[test]
    fn pure_bending_rate_value() {
        // k' = E·d⁴/(64·D·Na); E=203.4 GPa, d=2 mm, D=20 mm, Na=5.
        // = 203.4e9·1.6e-11/(64·0.02·5) = 3.2544/6.4 = 0.5085 N·m/rad (exact).
        let k = angular_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            5.0,
            FrictionModel::PureBending,
        );
        assert_relative_eq!(k.newton_meters_per_radian(), 0.5085, max_relative = 1e-12);
    }

    #[test]
    fn shigley_rate_is_softer_than_pure_bending() {
        // k' = E·d⁴/(2π·10.8·D·Na) = 3.2544/(67.85840132·0.02·5) = 3.2544/6.785840132
        //    = 0.47958689518357805 N·m/rad (softer than the 0.5085 pure-bending value).
        let k = angular_rate(
            Stress::from_pascals(203.4e9),
            Length::from_millimeters(2.0),
            Length::from_millimeters(20.0),
            5.0,
            FrictionModel::ShigleyFriction,
        );
        assert_relative_eq!(k.newton_meters_per_radian(), 0.47958689518357805, max_relative = 1e-9);
    }

    #[test]
    fn friction_model_default_is_shigley() {
        assert_eq!(FrictionModel::default(), FrictionModel::ShigleyFriction);
    }

    #[test]
    fn wound_diameter_shrinks_under_load() {
        // D' = D·N_b/(N_b + θ_turns); D=20 mm, N_b=5, θ=1 turn → 0.02·5/6 = 16.6667 mm.
        let dprime = wound_mean_diameter(Length::from_millimeters(20.0), 5.0, Angle::from_turns(1.0));
        assert_relative_eq!(dprime.millimeters(), 100.0 / 6.0, max_relative = 1e-12);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p springcore --lib torsion::mechanics`
Expected: FAIL (compile error — functions not defined).

- [ ] **Step 4: Implement the mechanics**

At the TOP of `springcore/src/torsion/mechanics.rs` (above the test module), add:

```rust
//! Torsion-spring mechanics: bending stress, active coils, angular rate, wind-up geometry.
//! All formulas cited inline (Shigley Ch. 10; EN 13906-3).

use crate::units::{Angle, AngularRate, Length, Moment, Stress};
use std::f64::consts::{PI, TAU};

/// Shigley's empirical per-turn rate denominator with inter-coil friction (Eq. 10-51).
const SHIGLEY_TURN_DENOM: f64 = 10.8;

/// Which angular-rate model the torsion solver uses. Selectable and (in a later GUI
/// phase) persisted, mirroring the shear-stress `CurvatureCorrection` precedent.
///
/// Deliberately NOT `#[non_exhaustive]`: `springcore` is an unpublished workspace crate
/// and the GUI will match this enum (variant → label), where a future variant should
/// force a compile error rather than a silent fallback (per the PR #32 scope decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrictionModel {
    /// Shigley Eq. 10-51 with empirical inter-coil friction (10.8 per turn). Default.
    #[default]
    ShigleyFriction,
    /// Pure-bending energy method (EN 13906-3; 64 per radian). No friction allowance.
    PureBending,
}

/// Inner-fiber bending stress-correction factor K_bi for round wire (Shigley Eq. 10-43):
/// `K_bi = (4C² − C − 1) / (4C(C − 1))`, where `C` is the spring index `D/d`. The inner
/// fiber carries the maximum bending stress and governs design.
pub fn kbi_factor(c: f64) -> f64 {
    (4.0 * c * c - c - 1.0) / (4.0 * c * (c - 1.0))
}

/// Nominal (uncorrected) bending stress `σ₀ = 32M/(πd³)` (Shigley Eq. 10-40 form).
pub fn bending_stress_nominal(moment: Moment, wire_dia: Length) -> Stress {
    let d = wire_dia.meters();
    Stress::from_pascals(32.0 * moment.newton_meters() / (PI * d.powi(3)))
}

/// Inner-fiber bending stress `σᵢ = K_bi · 32M/(πd³)` (Shigley Eq. 10-43), the critical
/// stress checked against the bending allowable.
pub fn bending_stress_inner(moment: Moment, mean_dia: Length, wire_dia: Length) -> Stress {
    let c = mean_dia.meters() / wire_dia.meters();
    Stress::from_pascals(kbi_factor(c) * bending_stress_nominal(moment, wire_dia).pascals())
}

/// Effective active coils including the straight-leg contribution (Shigley Eq. 10-50):
/// `Nₐ = N_b + (L₁ + L₂)/(3πD)`.
pub fn active_coils_with_legs(body_coils: f64, leg1: Length, leg2: Length, mean_dia: Length) -> f64 {
    let legs = leg1.meters() + leg2.meters();
    body_coils + legs / (3.0 * PI * mean_dia.meters())
}

/// Angular spring rate `k′ = M/θ` per radian.
///
/// - `PureBending` (EN 13906-3, energy method): `k′ = E·d⁴/(64·D·Nₐ)`.
/// - `ShigleyFriction` (Shigley Eq. 10-51): the 10.8-per-turn form with empirical
///   inter-coil friction, converted to per-radian: `k′ = E·d⁴/(2π·10.8·D·Nₐ)`.
pub fn angular_rate(
    youngs_modulus: Stress,
    wire_dia: Length,
    mean_dia: Length,
    active: f64,
    friction: FrictionModel,
) -> AngularRate {
    let e = youngs_modulus.pascals();
    let d = wire_dia.meters();
    let dm = mean_dia.meters();
    let denom_factor = match friction {
        FrictionModel::PureBending => 64.0,
        FrictionModel::ShigleyFriction => TAU * SHIGLEY_TURN_DENOM,
    };
    AngularRate::from_newton_meters_per_radian(e * d.powi(4) / (denom_factor * dm * active))
}

/// Wound-up mean diameter under load (Shigley Eq. 10-49): as the spring winds in the
/// load direction the body coils tighten, `D′ = D·N_b/(N_b + θ_turns)`.
pub fn wound_mean_diameter(mean_dia: Length, body_coils: f64, deflection: Angle) -> Length {
    let theta_turns = deflection.turns();
    Length::from_meters(mean_dia.meters() * body_coils / (body_coils + theta_turns))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore --lib torsion::mechanics`
Expected: PASS (9 tests).

- [ ] **Step 6: Gates**

Run: `cargo clippy -p springcore --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/lib.rs springcore/src/torsion/mod.rs springcore/src/torsion/mechanics.rs
git commit -m "feat(torsion): mechanics — K_bi bending stress, leg active coils, selectable rate, wind-up diameter"
```

---

### Task 3: Torsion static solver — types, solve_forward, status

**Files:**
- Create: `springcore/src/torsion/design.rs`
- Modify: `springcore/src/torsion/mod.rs` (add `mod design;` + re-exports)

**Interfaces:**
- Consumes: all of Task 2's mechanics; `Material`; `DesignStatus`, `Severity`, `StatusMessage` (crate root); `SpringError`, `Result`.
- Produces:
  - `pub struct TorsionInputs { pub wire_dia: Length, pub mean_dia: Length, pub body_coils: f64, pub leg1: Length, pub leg2: Length, pub arbor_dia: Option<Length> }`
  - `pub struct TorsionLoadPoint { pub moment: Moment, pub deflection: Angle, pub stress_inner: Stress, pub stress_nominal: Stress, pub pct_bending_allow: f64, pub wound_mean_dia: Length, pub wound_inner_dia: Length }`
  - `pub struct TorsionDesign { pub inputs: TorsionInputs, pub index: f64, pub active_coils: f64, pub rate: AngularRate, pub load_points: Vec<TorsionLoadPoint>, pub status: DesignStatus }`
  - `pub fn solve_forward(material: &Material, inputs: TorsionInputs, moments: &[Moment], friction: FrictionModel) -> Result<TorsionDesign>`

- [ ] **Step 1: Write failing solver tests**

Create `springcore/src/torsion/design.rs` with a test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::FrictionModel;
    use crate::units::{Length, Moment};
    use approx::assert_relative_eq;

    fn inputs() -> TorsionInputs {
        // d=2 mm, D=20 mm (C=10), N_b=5, no legs (Na=5), no arbor.
        TorsionInputs {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
        }
    }

    #[test]
    fn pure_bending_design_oracle() {
        // Music Wire E=203.4 GPa; PureBending k'=0.5085 N·m/rad (Task 2 oracle).
        // M=1 N·m → θ=1/0.5085=1.96656834 rad; θ_turns=0.31298907.
        // σᵢ = 389/360 × 1.2732395447e9 = 1.375806e9 Pa.
        // D' = 0.02·5/(5+0.31298907) = 0.0188218 m → inner' = 16.821797 mm.
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, inputs(), &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending).unwrap();
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
        assert_relative_eq!(d.active_coils, 5.0, max_relative = 1e-12);
        assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
        let lp = &d.load_points[0];
        assert_relative_eq!(lp.deflection.radians(), 1.9665683382497539, max_relative = 1e-9);
        assert_relative_eq!(lp.stress_inner.pascals(), (389.0 / 360.0) * 1.2732395447351628e9, max_relative = 1e-9);
        assert_relative_eq!(lp.stress_nominal.pascals(), 1.2732395447351628e9, max_relative = 1e-9);
        assert_relative_eq!(lp.wound_inner_dia.millimeters(), 16.821797, max_relative = 1e-4);
    }

    #[test]
    fn shigley_rate_matches_oracle() {
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, inputs(), &[Moment::from_newton_meters(1.0)], FrictionModel::ShigleyFriction).unwrap();
        assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.47958689518357805, max_relative = 1e-9);
    }

    #[test]
    fn pct_bending_allow_is_sigma_i_over_allowable() {
        // pct = σᵢ / (allowable_pct_bending · MTS(d)). Music Wire: pct_bending=0.75,
        // MTS(2mm)=2211·2^(−0.145) MPa = 2211·0.904181 = 1999.14 MPa.
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, inputs(), &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending).unwrap();
        let mts = m.min_tensile_strength(Length::from_millimeters(2.0)).unwrap().pascals();
        let expected = ((389.0 / 360.0) * 1.2732395447351628e9) / (0.75 * mts);
        assert_relative_eq!(d.load_points[0].pct_bending_allow, expected, max_relative = 1e-9);
    }

    #[test]
    fn overstress_raises_warning() {
        // A large moment drives σᵢ past the allowable → Warning status.
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, inputs(), &[Moment::from_newton_meters(50.0)], FrictionModel::PureBending).unwrap();
        assert!(d.status.has_warnings());
    }

    #[test]
    fn arbor_binding_raises_warning() {
        // Arbor diameter larger than the wound inner diameter → binding Warning.
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_millimeters(19.0)); // inner' ≈ 16.8 mm < 19 mm
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending).unwrap();
        assert!(d.status.has_warnings());
    }

    #[test]
    fn arbor_clear_no_binding_warning() {
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_millimeters(10.0)); // inner' ≈ 16.8 mm > 10 mm
        let m = crate::test_support::music_wire();
        let d = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending).unwrap();
        // No binding warning (a low moment also keeps stress under allowable).
        assert!(!d.status.has_warnings());
    }

    #[test]
    fn rejects_non_positive_wire_dia() {
        let mut i = inputs();
        i.wire_dia = Length::from_meters(0.0);
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_index_at_or_below_one() {
        let mut i = inputs();
        i.mean_dia = Length::from_millimeters(2.0); // D == d → C = 1
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_body_coils() {
        let mut i = inputs();
        i.body_coils = 0.0;
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_negative_leg() {
        let mut i = inputs();
        i.leg1 = Length::from_millimeters(-1.0);
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_moment() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, inputs(), &[Moment::from_newton_meters(0.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_empty_moments() {
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, inputs(), &[], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }

    #[test]
    fn rejects_non_positive_arbor() {
        let mut i = inputs();
        i.arbor_dia = Some(Length::from_meters(0.0));
        let m = crate::test_support::music_wire();
        let r = solve_forward(&m, i, &[Moment::from_newton_meters(1.0)], FrictionModel::PureBending);
        assert!(matches!(r, Err(crate::SpringError::InconsistentInputs(_))));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p springcore --lib torsion::design`
Expected: FAIL (compile error — types/fn not defined).

- [ ] **Step 3: Implement types, solver, and status**

At the TOP of `springcore/src/torsion/design.rs`, add:

```rust
//! Static forward solver for helical torsion springs. Validates geometry, computes the
//! angular rate, per-moment load points (deflection, bending stress, wound geometry),
//! and an engineering status. Formulas cited via `super::mechanics`.

use crate::design::{DesignStatus, Severity, StatusMessage};
use crate::material::Material;
use crate::torsion::mechanics::{
    active_coils_with_legs, angular_rate, bending_stress_inner, bending_stress_nominal,
    wound_mean_diameter, FrictionModel,
};
use crate::units::{Angle, AngularRate, Length, Moment, Stress};
use crate::{Result, SpringError};

/// Recommended spring-index band (SMI Handbook; Shigley §10-2), shared across families.
const INDEX_MIN: f64 = 4.0;
const INDEX_MAX: f64 = 12.0;

/// Geometry of a torsion spring. The legs are loaded so the coils wind tighter.
#[derive(Debug, Clone)]
pub struct TorsionInputs {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// Body (active) coil count `N_b`, excluding the leg contribution.
    pub body_coils: f64,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor (mandrel) diameter; when set, enables the wind-up clearance check.
    pub arbor_dia: Option<Length>,
}

/// One operating point: an applied moment and the resulting response.
#[derive(Debug, Clone)]
pub struct TorsionLoadPoint {
    /// Applied moment `M`.
    pub moment: Moment,
    /// Angular deflection `θ = M/k′`.
    pub deflection: Angle,
    /// Inner-fiber bending stress `σᵢ` (critical).
    pub stress_inner: Stress,
    /// Nominal bending stress `σ₀` (reference).
    pub stress_nominal: Stress,
    /// `σᵢ` as a fraction of the bending allowable (`allowable_pct_bending · MTS`).
    pub pct_bending_allow: f64,
    /// Wound-up mean diameter `D′` under this load.
    pub wound_mean_dia: Length,
    /// Wound-up inner diameter `D′ − d` under this load.
    pub wound_inner_dia: Length,
}

/// A fully solved torsion-spring design.
#[derive(Debug, Clone)]
pub struct TorsionDesign {
    /// The geometry that produced this design.
    pub inputs: TorsionInputs,
    /// Spring index `C = D/d`.
    pub index: f64,
    /// Effective active coils `Nₐ` (body + leg contribution).
    pub active_coils: f64,
    /// Angular rate `k′` (per radian).
    pub rate: AngularRate,
    /// One entry per applied moment.
    pub load_points: Vec<TorsionLoadPoint>,
    /// Engineering advisories (overstress, arbor binding, index range).
    pub status: DesignStatus,
}

/// Solve a torsion spring statically for one or more applied moments.
pub fn solve_forward(
    material: &Material,
    inputs: TorsionInputs,
    moments: &[Moment],
    friction: FrictionModel,
) -> Result<TorsionDesign> {
    let d = inputs.wire_dia.meters();
    if !(d.is_finite() && d > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "wire diameter must be a positive finite number".into(),
        ));
    }
    let dm = inputs.mean_dia.meters();
    if !(dm.is_finite() && dm > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must be a positive finite number".into(),
        ));
    }
    if dm <= d {
        return Err(SpringError::InconsistentInputs(
            "mean diameter must exceed wire diameter (spring index must exceed 1)".into(),
        ));
    }
    if !(inputs.body_coils.is_finite() && inputs.body_coils > 0.0) {
        return Err(SpringError::InconsistentInputs(
            "body coils must be a positive finite number".into(),
        ));
    }
    for leg in [inputs.leg1.meters(), inputs.leg2.meters()] {
        if !(leg.is_finite() && leg >= 0.0) {
            return Err(SpringError::InconsistentInputs(
                "leg lengths must be finite and non-negative".into(),
            ));
        }
    }
    if let Some(arbor) = inputs.arbor_dia {
        let a = arbor.meters();
        if !(a.is_finite() && a > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "arbor diameter must be a positive finite number".into(),
            ));
        }
    }
    if moments.is_empty() {
        return Err(SpringError::InconsistentInputs(
            "at least one applied moment is required".into(),
        ));
    }
    for m in moments {
        let mv = m.newton_meters();
        if !(mv.is_finite() && mv > 0.0) {
            return Err(SpringError::InconsistentInputs(
                "applied moment must be a positive finite number (load winds the coils tighter)"
                    .into(),
            ));
        }
    }
    // Validate the wire diameter against the material range (surfaces DiameterOutOfRange).
    let mts = material.min_tensile_strength(inputs.wire_dia)?.pascals();
    let allowable = material.allowable_pct_bending * mts;

    let index = dm / d;
    let active = active_coils_with_legs(
        inputs.body_coils,
        inputs.leg1,
        inputs.leg2,
        inputs.mean_dia,
    );
    let rate = angular_rate(
        material.youngs_modulus,
        inputs.wire_dia,
        inputs.mean_dia,
        active,
        friction,
    );

    let load_points: Vec<TorsionLoadPoint> = moments
        .iter()
        .map(|&moment| {
            let deflection =
                Angle::from_radians(moment.newton_meters() / rate.newton_meters_per_radian());
            let stress_inner = bending_stress_inner(moment, inputs.mean_dia, inputs.wire_dia);
            let stress_nominal = bending_stress_nominal(moment, inputs.wire_dia);
            let wound_mean_dia =
                wound_mean_diameter(inputs.mean_dia, inputs.body_coils, deflection);
            let wound_inner_dia = Length::from_meters(wound_mean_dia.meters() - d);
            TorsionLoadPoint {
                moment,
                deflection,
                stress_inner,
                stress_nominal,
                pct_bending_allow: stress_inner.pascals() / allowable,
                wound_mean_dia,
                wound_inner_dia,
            }
        })
        .collect();

    let status = evaluate_status(index, &load_points, inputs.arbor_dia);
    Ok(TorsionDesign {
        inputs,
        index,
        active_coils: active,
        rate,
        load_points,
        status,
    })
}

/// Engineering checks: overstress (inner-fiber), arbor binding under wind-up, index range.
fn evaluate_status(
    index: f64,
    load_points: &[TorsionLoadPoint],
    arbor_dia: Option<Length>,
) -> DesignStatus {
    let mut messages = Vec::new();

    if load_points.iter().any(|lp| lp.pct_bending_allow > 1.0) {
        messages.push(StatusMessage {
            severity: Severity::Warning,
            message: "inner-fiber bending stress exceeds the allowable".into(),
        });
    }
    if let Some(arbor) = arbor_dia {
        if load_points
            .iter()
            .any(|lp| lp.wound_inner_dia.meters() <= arbor.meters())
        {
            messages.push(StatusMessage {
                severity: Severity::Warning,
                message: "spring winds down onto the arbor (inner diameter binds)".into(),
            });
        }
    }
    if index < INDEX_MIN || index > INDEX_MAX {
        messages.push(StatusMessage {
            severity: Severity::Caution,
            message: format!(
                "spring index {index:.2} is outside the recommended range {INDEX_MIN}–{INDEX_MAX}"
            ),
        });
    }

    DesignStatus { messages }
}
```

- [ ] **Step 4: Wire the module re-exports**

In `springcore/src/torsion/mod.rs`, add `mod design;` after `mod mechanics;` and extend the re-exports:

```rust
mod design;
mod mechanics;

pub use design::{solve_forward, TorsionDesign, TorsionInputs, TorsionLoadPoint};
pub use mechanics::{
    active_coils_with_legs, angular_rate, bending_stress_inner, bending_stress_nominal,
    kbi_factor, wound_mean_diameter, FrictionModel,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore --lib torsion::design`
Expected: PASS (13 tests).

- [ ] **Step 6: Gates**

Run: `cargo clippy -p springcore --all-targets -- -D warnings && cargo test -p springcore`
Expected: no warnings; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/torsion/design.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): static solver — load points, bending-allowable + arbor status, validation"
```

---

### Task 4: PowerUser scenario + public surface

**Files:**
- Create: `springcore/src/torsion/scenario.rs`
- Modify: `springcore/src/torsion/mod.rs` (add `mod scenario;` + re-exports)

**Interfaces:**
- Consumes: `solve_forward`, `TorsionInputs`, `TorsionDesign`, `FrictionModel` (Task 3/2); `Material`; `Length`, `Moment`.
- Produces:
  - `pub trait Scenario { fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign>; }`
  - `pub struct PowerUser { pub wire_dia: Length, pub mean_dia: Length, pub body_coils: f64, pub leg1: Length, pub leg2: Length, pub arbor_dia: Option<Length>, pub moments: Vec<Moment> }`

- [ ] **Step 1: Write the failing scenario test**

Create `springcore/src/torsion/scenario.rs` with a test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::torsion::FrictionModel;
    use crate::units::{Length, Moment};
    use approx::assert_relative_eq;

    #[test]
    fn power_user_solves_to_same_design_as_solve_forward() {
        let m = crate::test_support::music_wire();
        let pu = PowerUser {
            wire_dia: Length::from_millimeters(2.0),
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        };
        let d = pu.solve(&m, FrictionModel::PureBending).unwrap();
        assert_relative_eq!(d.rate.newton_meters_per_radian(), 0.5085, max_relative = 1e-9);
        assert_eq!(d.load_points.len(), 1);
        assert_relative_eq!(d.index, 10.0, max_relative = 1e-12);
    }

    #[test]
    fn power_user_propagates_validation_errors() {
        let m = crate::test_support::music_wire();
        let pu = PowerUser {
            wire_dia: Length::from_meters(0.0), // invalid
            mean_dia: Length::from_millimeters(20.0),
            body_coils: 5.0,
            leg1: Length::from_meters(0.0),
            leg2: Length::from_meters(0.0),
            arbor_dia: None,
            moments: vec![Moment::from_newton_meters(1.0)],
        };
        assert!(matches!(
            pu.solve(&m, FrictionModel::PureBending),
            Err(crate::SpringError::InconsistentInputs(_))
        ));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: FAIL (compile error — `PowerUser`/`Scenario` not defined).

- [ ] **Step 3: Implement the scenario**

At the TOP of `springcore/src/torsion/scenario.rs`, add:

```rust
//! Determined solve scenarios for torsion springs. Each scenario is a fixed assignment
//! of which quantities are inputs; it delegates to `design::solve_forward`.

use crate::material::Material;
use crate::torsion::design::{solve_forward, TorsionDesign, TorsionInputs};
use crate::torsion::mechanics::FrictionModel;
use crate::units::{Length, Moment};
use crate::Result;

/// A solve scenario for torsion springs.
pub trait Scenario {
    /// Compute a complete torsion-spring design for this scenario's inputs.
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign>;
}

/// All geometry given; compute performance. The torsion counterpart to the
/// compression/extension `PowerUser` scenario.
#[derive(Debug, Clone)]
pub struct PowerUser {
    /// Wire diameter `d`.
    pub wire_dia: Length,
    /// Mean coil diameter `D`.
    pub mean_dia: Length,
    /// Body coil count `N_b`.
    pub body_coils: f64,
    /// First straight-leg length `L₁`.
    pub leg1: Length,
    /// Second straight-leg length `L₂`.
    pub leg2: Length,
    /// Optional arbor diameter for the wind-up clearance check.
    pub arbor_dia: Option<Length>,
    /// Applied moments (one load point each).
    pub moments: Vec<Moment>,
}

impl Scenario for PowerUser {
    fn solve(&self, material: &Material, friction: FrictionModel) -> Result<TorsionDesign> {
        solve_forward(
            material,
            TorsionInputs {
                wire_dia: self.wire_dia,
                mean_dia: self.mean_dia,
                body_coils: self.body_coils,
                leg1: self.leg1,
                leg2: self.leg2,
                arbor_dia: self.arbor_dia,
            },
            &self.moments,
            friction,
        )
    }
}
```

- [ ] **Step 4: Wire module re-exports**

In `springcore/src/torsion/mod.rs`, add `mod scenario;` and re-export:

```rust
pub use scenario::{PowerUser, Scenario};
```

Do NOT add crate-root re-exports in `lib.rs`. The `extension` family is accessed
module-qualified (`springcore::extension::*`) with no crate-root `pub use`; torsion
mirrors that exactly — all torsion types are reached via `springcore::torsion::*`
(e.g. `springcore::torsion::{solve_forward, PowerUser, Scenario, FrictionModel,
TorsionDesign}`). This also avoids clashing with the compression `PowerUser`/`Scenario`/
`solve_forward` already re-exported at the crate root. `lib.rs` already gained
`pub mod torsion;` in Task 2.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p springcore --lib torsion::scenario`
Expected: PASS (2 tests).

- [ ] **Step 6: Full gate**

```bash
cargo fmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
typos
cargo deny check all
git diff origin/main -- > /tmp/torsion.diff
cargo mutants --in-diff /tmp/torsion.diff --no-shuffle -j 2 --package springcore --all-features
```
Expected: all green; mutation **0 survivors**. If a genuinely-new survivor appears, kill it with a boundary test; only exclude a mutant in `.cargo/mutants.toml` with a formal per-entry equivalence argument.

- [ ] **Step 7: Commit**

```bash
git add springcore/src/torsion/scenario.rs springcore/src/torsion/mod.rs
git commit -m "feat(torsion): PowerUser scenario and public surface"
```

---

## Self-Review (completed during planning)

**Spec coverage:** new units (Task 1) ✓; mechanics incl. K_bi/legs/both rate models/wound diameter (Task 2) ✓; static solver + inner-fiber stress + nominal + bending-allowable check + arbor binding + index caution (Task 3) ✓; PowerUser scenario (Task 4) ✓; golden/oracle + validation + conversion tests across tasks ✓; mutation gate (Task 4 Step 6) ✓; `allowable_pct_bending` reused, no new material field ✓; `FrictionModel` not `#[non_exhaustive]` ✓.

**Deferred per spec (not in this plan):** GUI, additional input modes, force-on-leg input, OD-as-input convenience, optimizer, fatigue.

**Type consistency:** `FrictionModel`, `TorsionInputs`, `TorsionLoadPoint`, `TorsionDesign`, `solve_forward(material, inputs, moments, friction)`, and `PowerUser { ..., moments: Vec<Moment> }` are used identically across Tasks 2–4. Unit constructor/accessor names match Task 1 throughout.

**Note on the Shigley published example:** the automated correctness anchors are hand-derived oracles computed directly from the cited Shigley/EN formulas (values shown inline with their arithmetic). A reviewer wanting an additional published cross-check may transcribe Shigley Ex. 10-7/10-8 from the textbook PDF and pin it with a ~2% tolerance, but that is optional hardening, not required for Phase 1.
