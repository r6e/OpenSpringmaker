//! 2D engineering-diagram visual mode (ADR 0008): pure projection
//! (`geometry`) + pure layout (`layout`) feeding the humble `canvas`.
pub mod geometry;

// Re-exports consumed by a later diagram task (layout + humble canvas); Task 1
// ships the projection API ahead of its first caller.
#[allow(unused_imports)]
pub use geometry::{project_silhouette, Bounds, Edge2, Projected, P2};
