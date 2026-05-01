//! Run lifecycle: prepare, capture (the agent's job), finalize, abort,
//! resume; plus chain verification.
//!
//! The seam is the directory layout under `evidence/<y>/<q>/<id>/<run-id>/`
//! described in `docs/storage.md`. The binary owns every state-changing
//! filesystem operation; the agent only writes data files into the slots
//! prepare carved out.

pub mod hasher;
pub mod lock;
pub mod manifest;
pub mod runner;
pub mod verifier;
