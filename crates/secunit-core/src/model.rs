//! Strongly-typed registry model.
//!
//! Types here mirror the on-disk YAML/JSON shapes documented in
//! `docs/storage.md` and validated by the schemas under `schemas/`. Keep
//! field names aligned with the schemas — `serde` deserialisation is the
//! load path used by `registry::loader`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ---------- shared primitives -----------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cadence {
    Continuous,
    Weekly,
    Monthly,
    Quarterly,
    #[serde(rename = "semi-annual")]
    SemiAnnual,
    Annual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub fn to_chrono(self) -> chrono::Weekday {
        match self {
            Weekday::Monday => chrono::Weekday::Mon,
            Weekday::Tuesday => chrono::Weekday::Tue,
            Weekday::Wednesday => chrono::Weekday::Wed,
            Weekday::Thursday => chrono::Weekday::Thu,
            Weekday::Friday => chrono::Weekday::Fri,
            Weekday::Saturday => chrono::Weekday::Sat,
            Weekday::Sunday => chrono::Weekday::Sun,
        }
    }
}

// ---------- control ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Control {
    pub id: String,
    pub title: String,
    pub policy: String,
    #[serde(default)]
    pub nist: Vec<String>,
    pub owner: String,
    pub cadence: Cadence,
    #[serde(default)]
    pub weekday: Option<Weekday>,
    #[serde(default)]
    pub due_by: Option<String>,
    pub skill: String,
    #[serde(default)]
    pub skill_args: Option<serde_json::Value>,
    #[serde(default)]
    pub scope: Option<Scope>,
    #[serde(default)]
    pub evidence_required: Vec<EvidenceRequirement>,
    #[serde(default)]
    pub remediation_thresholds: BTreeMap<String, u32>,
    #[serde(default)]
    pub outputs: Option<serde_json::Value>,
    #[serde(default)]
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub title: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirement {
    pub kind: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub per_system: Option<bool>,
}

// ---------- scope -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scope {
    Inventory(InventoryScope),
    Inline(InlineScope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryScope {
    pub kind: String,
    #[serde(default)]
    pub all: Option<bool>,
    #[serde(default)]
    pub has_tags: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineScope {
    pub inline: Vec<InlineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineEntry {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ---------- inventory -------------------------------------------------------

/// Inventory is a top-level map keyed by `kind` → `Vec<Entry>`. Kinds are
/// free-form; the conventional set is documented in `docs/storage.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Inventory {
    pub kinds: BTreeMap<String, Vec<InventoryEntry>>,
}

impl Inventory {
    /// Inventory sections are conventionally plural (`source_repos`,
    /// `cloud_accounts`) but `scope.kind` is singular (`source_repo`,
    /// `cloud_account`). Look up by exact match first, then by trailing-`s`
    /// pluralisation.
    pub fn entries(&self, kind: &str) -> &[InventoryEntry] {
        if let Some(v) = self.kinds.get(kind) {
            return v;
        }
        let plural = format!("{kind}s");
        if let Some(v) = self.kinds.get(&plural) {
            return v;
        }
        &[]
    }

    /// Returns the canonical section name (as it appears in YAML) that
    /// satisfies the given scope kind, if any. Used by validation to tell
    /// "section is missing" from "kind is genuinely unknown".
    pub fn section_for(&self, kind: &str) -> Option<&str> {
        if let Some((k, _)) = self.kinds.get_key_value(kind) {
            return Some(k.as_str());
        }
        let plural = format!("{kind}s");
        self.kinds.get_key_value(&plural).map(|(k, _)| k.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &InventoryEntry)> {
        self.kinds
            .iter()
            .flat_map(|(k, v)| v.iter().map(move |e| (k, e)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub in_scope_since: Option<NaiveDate>,
    #[serde(default)]
    pub retired_on: Option<NaiveDate>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Skill names this entry opts out of, even if the tag filter matches.
    #[serde(default)]
    pub excludes: Vec<String>,
    /// Anything else from the YAML — `url`, `stack`, `provider`, `profile`,
    /// `owner`, `address`, etc. — surfaced to skills as-is.
    #[serde(flatten)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

impl InventoryEntry {
    /// Active on `date` according to lifecycle dates. `in_scope_since` is
    /// inclusive; `retired_on` is exclusive (per `storage.md`).
    pub fn is_active_on(&self, date: NaiveDate) -> bool {
        if let Some(start) = self.in_scope_since {
            if date < start {
                return false;
            }
        }
        if let Some(end) = self.retired_on {
            if date >= end {
                return false;
            }
        }
        true
    }
}

// ---------- schedule --------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub overrides: Vec<ScheduleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub control_id: String,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    #[serde(default)]
    pub weekday: Option<Weekday>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub skip: Option<ScheduleSkip>,
    #[serde(default)]
    pub insert: Option<ScheduleInsert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSkip {
    #[serde(default)]
    pub quarter: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleInsert {
    pub run_at: NaiveDate,
    #[serde(default)]
    pub reason: Option<String>,
}

// ---------- state -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub schema_version: u32,
    #[serde(default)]
    pub controls: BTreeMap<String, StateEntry>,
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: crate::SCHEMA_VERSION,
            controls: BTreeMap::new(),
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateEntry {
    pub last_run_id: Option<String>,
    pub last_run_path: Option<String>,
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_status: RunStatus,
    pub next_due: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Complete,
    InProgress,
    Failed,
    NeverRun,
}

// ---------- config ----------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub org: Option<OrgConfig>,
    #[serde(default)]
    pub owners: BTreeMap<String, String>,
    #[serde(default)]
    pub weekly_default_weekday: Option<Weekday>,
    #[serde(default)]
    pub thresholds: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub integrations: BTreeMap<String, serde_json::Value>,
    /// Remaining org-specific fields surfaced to skills as-is.
    #[serde(flatten)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub wisp_repo: Option<String>,
    #[serde(flatten)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

// ---------- resolved scope --------------------------------------------------

/// One entry produced by scope resolution. Carries enough metadata to be
/// embedded into `prepare.json` and `manifest.json` directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedSystem {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Inventory-side fields (url, stack, provider, ...) preserved for
    /// skill consumption.
    #[serde(flatten)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

// ---------- registry container ---------------------------------------------

/// The fully-loaded org tree, in memory.
#[derive(Debug, Clone)]
pub struct LoadedRegistry {
    pub root: PathBuf,
    pub controls: BTreeMap<String, Control>,
    pub inventory: Inventory,
    pub schedule: Schedule,
    pub state: State,
    pub config: Config,
}
