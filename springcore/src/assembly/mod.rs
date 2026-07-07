//! Assemblies of cylindrical round-wire compression springs — Nested
//! (concentric, parallel-acting) or Series (stacked).
//!
//! PURE COMPOSITION: every member is solved by the existing cited
//! compression engine (`crate::design::solve_forward`); this module adds
//! only the combination layer. Rate/load-sharing sources: Shigley 10th ed.
//! Ch. 4's worked nested-pair example (k = Σkᵢ, Fᵢ = kᵢF/Σk) and
//! Eq. 8-15 / Prob. 4-1 (1/k = Σ 1/kᵢ for series), each generalized to N
//! members by the same equilibrium argument (derivation notes at the
//! formula sites). §10-1 endorses nested round-wire springs explicitly.
//!
//! HONEST BOUNDARY: nested members must share a free length — staged
//! engagement (members engaging at different deflections) is
//! progressive-contact physics with no in-house citation, the same class
//! excluded for variable pitch and conical's post-bottoming regime.
//!
//! Deliberate omissions (none fabricated): opposite-hand winding
//! convention for adjacent nested members (industry practice, not in
//! Shigley); stack-level buckling for series (per-member stability flags
//! still surface, member-indexed); assembly-level surge frequency.

mod design;

pub use design::{
    evaluate_status, solve_assembly, AssemblyDesign, AssemblyInputs, AssemblyLoadPoint,
    AssemblyMember, MemberResult, Topology,
};
