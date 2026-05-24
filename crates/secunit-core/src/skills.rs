//! Bundled standard-library skills + the one resolver every skill
//! reference goes through.
//!
//! A skill name resolves the same way everywhere it appears — a control's
//! `skill:`, a runbook's `skill_args.extend:`, `validate`, `run prepare`,
//! and `secunit skills show`: org-local `<root>/skills/<name>.md` first
//! (the override), then the copy bundled into this binary. Bundled skills
//! ship with the release, so an org needs no install step; it overrides a
//! skill — spine or fragment — by dropping a same-named file under
//! `skills/`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::evidence::hasher::sha256_bytes;

/// A skill embedded into the binary at compile time.
pub struct BundledSkill {
    pub name: &'static str,
    pub body: &'static str,
}

macro_rules! bundled {
    ($name:literal) => {
        BundledSkill {
            name: $name,
            // Relative to this file (crates/secunit-core/src/skills.rs);
            // the workspace `skills/` dir is three levels up.
            body: include_str!(concat!("../../../skills/", $name, ".md")),
        }
    };
}

/// The standard library. Generic, org-agnostic runbooks; org specifics
/// flow in through control `skill_args` and `_config.yaml`, never through
/// the skill text. Keep this list in sync with `skills/`.
pub const BUNDLED: &[BundledSkill] = &[
    bundled!("capture-sweep"),
    bundled!("attestation-review"),
    bundled!("policy-annual-review"),
    bundled!("report"),
    bundled!("bootstrap"),
    bundled!("inventory-seed"),
];

/// Where a resolved skill came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    /// An org-local file at `<root>/skills/<name>.md` (overrides bundled).
    Local,
    /// A copy compiled into this binary.
    Bundled,
}

impl SkillSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillSource::Local => "local",
            SkillSource::Bundled => "bundled",
        }
    }
}

/// A skill resolved by name, carrying the sha256 that pins it into a run
/// manifest.
#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub name: String,
    pub body: String,
    pub source: SkillSource,
    /// The file path for a `Local` skill; `None` when bundled.
    pub path: Option<PathBuf>,
    pub sha256: String,
}

/// Resolve a skill by name: org-local file first, then the bundled copy.
/// `None` if neither has it.
pub fn resolve(root: &Path, name: &str) -> Option<ResolvedSkill> {
    let local = root.join("skills").join(format!("{name}.md"));
    if local.is_file() {
        // Read once: the sha is over the raw bytes (matching the manifest's
        // artifact-hashing convention and any chain sealed before bundling),
        // and the body is decoded from those same bytes. If the file can't be
        // read, warn and fall through to bundled rather than returning a
        // skill with a silently-empty body.
        match std::fs::read(&local) {
            Ok(bytes) => {
                return Some(ResolvedSkill {
                    name: name.to_string(),
                    sha256: sha256_bytes(&bytes),
                    body: String::from_utf8_lossy(&bytes).into_owned(),
                    source: SkillSource::Local,
                    path: Some(local),
                });
            }
            Err(e) => {
                tracing::warn!(
                    path = %local.display(),
                    error = %e,
                    "skill `{name}` exists locally but could not be read; falling back to bundled"
                );
            }
        }
    }
    BUNDLED
        .iter()
        .find(|s| s.name == name)
        .map(|s| ResolvedSkill {
            name: name.to_string(),
            body: s.body.to_string(),
            source: SkillSource::Bundled,
            path: None,
            sha256: sha256_bytes(s.body.as_bytes()),
        })
}

/// True if `name` resolves to anything (org-local or bundled).
pub fn exists(root: &Path, name: &str) -> bool {
    root.join("skills").join(format!("{name}.md")).is_file()
        || BUNDLED.iter().any(|s| s.name == name)
}

/// Pull the YAML frontmatter block out of a skill markdown body. Returns
/// `None` if the body does not start with `---\n`.
pub fn frontmatter(body: &str) -> Option<&str> {
    let rest = body.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

/// The one-line `description:` from a skill's frontmatter, if present.
pub fn description(body: &str) -> Option<String> {
    let fm = frontmatter(body)?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(fm).ok()?;
    parsed
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// The `requires_features:` list from a skill's frontmatter, if present.
pub fn requires_features(body: &str) -> Vec<String> {
    let Some(fm) = frontmatter(body) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(fm) else {
        return Vec::new();
    };
    parsed
        .get("requires_features")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_skill_has_matching_frontmatter_name() {
        for s in BUNDLED {
            let fm = frontmatter(s.body)
                .unwrap_or_else(|| panic!("bundled skill {} lacks frontmatter", s.name));
            let parsed: serde_yaml::Value = serde_yaml::from_str(fm).unwrap();
            assert_eq!(
                parsed.get("name").and_then(|v| v.as_str()),
                Some(s.name),
                "bundled skill `{}` frontmatter name must match its registry key",
                s.name
            );
        }
    }

    #[test]
    fn resolve_falls_back_to_bundled() {
        // A fresh temp dir has no skills/, so resolution must hit the bundle.
        let tmp = tempfile::tempdir().unwrap();
        let r = resolve(tmp.path(), "capture-sweep").expect("capture-sweep is bundled");
        assert_eq!(r.source, SkillSource::Bundled);
        assert!(!r.sha256.is_empty());
        assert!(r.body.contains("# Capture sweep"));
    }

    #[test]
    fn resolve_local_overrides_bundled() {
        // A local file shadows the bundled skill of the same name, and its
        // sha is over the file's raw bytes.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("skills")).unwrap();
        let body = "---\nname: capture-sweep\n---\n# local override\n";
        std::fs::write(tmp.path().join("skills/capture-sweep.md"), body).unwrap();
        let r = resolve(tmp.path(), "capture-sweep").expect("resolves locally");
        assert_eq!(r.source, SkillSource::Local);
        assert_eq!(r.body, body);
        assert_eq!(r.sha256, sha256_bytes(body.as_bytes()));
    }

    #[test]
    fn resolve_unknown_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve(tmp.path(), "no-such-skill").is_none());
    }

    #[test]
    fn frontmatter_extract_basic() {
        let body = "---\nname: x\nrequires_features: [a, b]\n---\n# body";
        assert_eq!(requires_features(body), vec!["a", "b"]);
    }
}
