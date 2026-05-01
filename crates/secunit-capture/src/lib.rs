//! `secunit-capture` — native upstream capturers (AWS, GitHub, dependency
//! audits, generic HTTP). Each subsystem is gated behind a cargo feature
//! so operators install only what they need. Phase 4+ fills these in.

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
