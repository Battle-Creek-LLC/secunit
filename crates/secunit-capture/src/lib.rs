//! `secunit-capture` — native upstream capturers (AWS, GitHub, dependency
//! audits, generic HTTP).
//!
//! Every capturer writes a canonical envelope shaped
//! `{ capturer, version, captured_at, args, result }` with sorted arrays,
//! ISO-8601 UTC timestamps, and ephemeral fields (request ids, pagination
//! tokens) stripped, so two runs of the same fixture round-trip
//! byte-identically.

pub mod canonical;
pub mod schema;
pub mod time;

/// CycloneDX BOM normalization and schema validation. Gated with the
/// SBOM capturer that produces the BOMs, since the vendored CycloneDX
/// schemas are ~500KB of `include_str!` that a `--no-default-features`
/// build has no reason to carry.
#[cfg(feature = "deps")]
pub mod cyclonedx;

#[cfg(feature = "deps")]
pub mod deps;

#[cfg(feature = "github")]
pub mod github;

/// Compile-time list of features actually enabled in this build.
pub fn enabled_features() -> &'static [&'static str] {
    &[
        #[cfg(feature = "aws")]
        "aws",
        #[cfg(feature = "github")]
        "github",
        #[cfg(feature = "deps")]
        "deps",
        #[cfg(feature = "http")]
        "http",
        #[cfg(feature = "gcp")]
        "gcp",
    ]
}
