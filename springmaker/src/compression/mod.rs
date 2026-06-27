//! Compression-spring calculator screen: form (input parsing and solving),
//! view-model (the compression presenter functions and result aggregates), and
//! view (the humble iced widget tree). Mirrors the engine's per-family layout;
//! shared vocabulary lives in `crate::presenter` and `crate::widgets`.

pub(crate) mod form;
pub(crate) mod view;
pub(crate) mod view_model;
