//! Registry loading + cadence/scope resolution.
//!
//! The loader walks an org root, parses every YAML/JSON it expects, and
//! returns a `LoadedRegistry` plus a per-file diagnostic report. The
//! resolver answers "when is this control next due?" and "what systems
//! does its scope expand to on this date?" — both pure functions over the
//! loaded model so they're easy to property-test.

pub mod loader;
pub mod resolver;
