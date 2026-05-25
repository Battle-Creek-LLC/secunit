//! IPC commands callable from the webview. Read-only by design; see
//! `JOB-13-readonly-audit.md` for the audit checklist.

pub mod projects_cmd;
pub mod registry_cmd;
pub mod risks_cmd;
pub mod types;

pub use projects_cmd::*;
pub use registry_cmd::*;
pub use risks_cmd::*;
