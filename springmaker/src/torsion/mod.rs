//! Torsion-spring Calculator GUI: form, view-model (pure presenter),
//! plot_model (chart presenter), scene_model (3D scene presenter),
//! diagram_model (2D engineering-diagram dimension + end-on inset presenter),
//! and humble view.
pub(crate) mod diagram_model;
pub mod form;
pub(crate) mod plot_model;
pub(crate) mod scene_model;
pub mod view;
pub mod view_model;
