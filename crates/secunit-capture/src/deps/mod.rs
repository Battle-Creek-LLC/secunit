//! Source-side dependency-audit capturers.
//!
//! Four actions, one canonical envelope shape each. Library APIs where
//! practical (`rustsec` for cargo, REST for OSV); subprocess fallback
//! for pip and pnpm where no Rust-callable library exists.
//!
//! The cargo-audit capturer runs `rustsec` without its `git`/`gix`
//! features (those pull a vulnerable `gix` tree): the advisory db is
//! either supplied via `--db-path` / `SECUNIT_RUSTSEC_DB` or downloaded
//! over plain HTTPS as a tarball — never via the `gix` git client.

pub mod cargo_audit;
pub mod cmd;
pub mod osv_query;
pub mod pip_audit;
pub mod pnpm_audit;
