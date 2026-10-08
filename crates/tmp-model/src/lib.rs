//! Toolkit-agnostic model for the TaskManager+ Lite ports.
//!
//! Every frontend in `frontends/` renders from this crate, so the ports differ
//! only in UI code. They share the telemetry, grouping, sort order, derived
//! numbers, and poll cadence. That sharing is what makes the benchmark
//! comparison fair.
//!
//! - A [`Source`] produces one [`Sample`] per tick. [`Synthetic`] is the only
//!   source today. A live source over `taskmanagerplus_lib::ffi` comes later
//!   once the telemetry layer is extracted from TaskManagerPlus.
//! - [`spawn_poller`] runs a source on a background thread, folds each sample
//!   into a shared [`Model`], then calls the frontend's wake callback.
//!   Frontends repaint from that callback only, never on a free-running frame
//!   loop.
//! - [`groups`] holds the Processes page logic, lifted from
//!   `lite/processes.rs`.
//! - [`fmt`], [`derived`] and [`palette`] hold the shared formatting, the
//!   math, and the series colors.

pub mod derived;
pub mod fmt;
pub mod groups;
pub mod palette;
pub mod settings;

mod model;
mod source;
mod synthetic;
mod types;

pub use model::{MAX_INTERVAL_MS, MIN_INTERVAL_MS, Model, Poller, SharedModel, spawn_poller};
pub use source::{Sample, Source};
pub use synthetic::{BatteryStart, Synthetic, SyntheticConfig};
pub use types::{CoreCpu, History, Proc, Series, Snapshot};
