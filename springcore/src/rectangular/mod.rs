//! Rectangular- (and square-) wire helical compression springs.
//!
//! **Model.** The wire is treated as a bar in torsion (Shigley 10th ed. §3-14).
//! The rate assembles Eq. 3-41 (angle of twist of a rectangular bar,
//! `θ = T·l/(β·b·c³·G)`) with close-coiled helix geometry into
//! `k = 4·β·b·c³·G / (π·D³·n)`; the max shear stress assembles Eq. 3-40
//! (`τ₀ = T/(α·b·c²)`) with the selectable Wahl/Bergsträsser curvature
//! correction `K(C)`, `C = D/b`. The section is oriented `b = max(axial, radial)`,
//! `c = min(...)` (Shigley's convention — `b` is the longer side), so `b/c ≥ 1`.
//! α and β are linearly interpolated from Shigley's tabulated coefficients and
//! clamped above `b/c = 10` (conservative — the true section is stiffer and
//! slightly less stressed; surfaced as an Info status).
//!
//! For a **square** section the model reproduces the Air Force Stress Analysis
//! Manual (AFFDL, Oct 1986) §1.5.4.2 formulas: the rate matches Eq. 1-90
//! (`2π/0.141 = 44.56 ≈ 44.5`) and the stress matches Eq. 1-84
//! (`1/0.208 = 4.808 ≈ 4.80`). Two independent authorities agreeing to 3 sig figs.
//!
//! **Deliberate omissions** (none fabricated):
//! - **Buckling** — the coil-column criterion needs an end-fixity input that
//!   `RectangularInputs` does not carry; deferred to the GUI increment.
//!   (Conical likewise omits buckling, though for its own reason — see that
//!   module's docs.)
//! - **Natural frequency** — the cylindrical surge formula assumes a round
//!   section; no cited rectangular-wire replacement in-house.

mod design;

pub use design::{evaluate_status, solve_forward, RectangularDesign, RectangularInputs};
