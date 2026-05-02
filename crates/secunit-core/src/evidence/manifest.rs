//! Strongly-typed prepare / result / manifest payloads. Each maps 1:1
//! to its JSON Schema under `schemas/`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::ResolvedSystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScopeLayout {
    BySystem,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunOutcome {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemOutcome {
    Complete,
    Skipped,
    Failed,
}

// ---------- prepare context -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareContext {
    pub schema_version: u32,
    pub control_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub scope_layout: ScopeLayout,
    pub resolved_scope: Vec<ResolvedSystem>,
    pub registry_git_sha: String,
}

// ---------- result (skill output) ------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub schema_version: u32,
    pub control_id: String,
    pub run_id: String,
    pub status: RunOutcome,
    #[serde(default)]
    pub by_system: Vec<SystemResult>,
    #[serde(default)]
    pub draft_risks: Vec<serde_json::Value>,
    #[serde(default)]
    pub draft_issues: Vec<serde_json::Value>,
    #[serde(default)]
    pub external_links: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResult {
    pub name: String,
    pub status: SystemOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------- finalized manifest ---------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub model: String,
    pub skill: String,
    pub skill_sha256: String,
    pub control_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorRun {
    pub run_id: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BySystemBlock {
    pub name: String,
    pub status: SystemOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<BTreeMap<String, serde_json::Value>>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub control_id: String,
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    pub agent: AgentInfo,
    pub registry_git_sha: String,
    pub scope_layout: ScopeLayout,
    pub resolved_scope: Vec<ResolvedSystem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_run: Option<PriorRun>,
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_system: Vec<BySystemBlock>,
    pub status: RunOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub draft_risks: Vec<serde_json::Value>,
    #[serde(default)]
    pub draft_issues: Vec<serde_json::Value>,
    #[serde(default)]
    pub external_links: Vec<serde_json::Value>,
}
