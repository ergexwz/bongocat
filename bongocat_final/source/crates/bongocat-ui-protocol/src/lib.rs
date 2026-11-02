#![forbid(unsafe_code)]

//! Typed contracts shared by the settings/update services and their UI views.
//!
//! This crate deliberately has no GPUI, operating-system, platform,
//! localization, runtime, model, or update-library dependency. It owns the stable
//! command/snapshot/error shapes
//! and the bounded channel clients; concrete application services and GPUI
//! views adapt those contracts at their own boundaries.
//!
//! `bongocat-config` supplies the shared render-quality enum.
//! `bongocat-input` supplies
//! [`ModifierKey`]: the setting the overlay page records and renders is a
//! physical key, and re-spelling it here would let the value the window sends
//! drift from the usage the runtime matches it against.

pub use bongocat_config::RenderQuality;

mod settings;
mod update;

pub use settings::*;
pub use update::*;
