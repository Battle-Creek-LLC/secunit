//! Source-side dependency-audit capturers.
//!
//! Four actions, one canonical envelope shape each. Library APIs where
//! practical (`rustsec` for cargo, REST for OSV); subprocess fallback
//! for pip and pnpm where no Rust-callable library exists.

pub mod cargo_audit;
pub mod cmd;
pub mod osv_query;
pub mod pip_audit;
pub mod pnpm_audit;
