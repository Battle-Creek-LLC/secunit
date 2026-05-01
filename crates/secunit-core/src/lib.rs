//! `secunit-core` — registry parsing, cadence/scope resolution, evidence
//! hashing, and manifest verification. Library-shaped so tests and the CLI
//! can drive it without going through `clap`.

pub mod evidence;
pub mod model;
pub mod registry;
pub mod schemas;

/// Schema version implemented by this crate. Bumped only on breaking
/// on-disk changes; reads of older versions remain best-effort.
pub const SCHEMA_VERSION: u32 = 1;
