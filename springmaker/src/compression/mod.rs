//! Compression-spring calculator screen: form (input parsing and solving),
//! view-model (the compression presenter functions and result aggregates),
//! plot_model (the chart presenter), scene_model (the 3D scene presenter),
//! diagram_model (the 2D engineering-diagram dimension presenter), and view
//! (the humble iced widget tree). Mirrors the engine's per-family layout;
//! shared vocabulary lives in `crate::presenter` and `crate::widgets`.

pub(crate) mod diagram_model;
pub(crate) mod form;
pub(crate) mod plot_model;
pub(crate) mod scene_model;
pub(crate) mod view;
pub(crate) mod view_model;
