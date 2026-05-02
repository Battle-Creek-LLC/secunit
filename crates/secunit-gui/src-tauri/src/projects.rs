//! Read `~/.config/secunit-gui/projects.yaml` and persist the
//! last-selected project in a sibling `state.json`.
//!
//! The GUI never modifies anything inside a project tree, but it does
//! read and (for the last-selected pointer) write inside its own config
//! directory. That is the only on-disk state the GUI owns.
//!
//! ```yaml
//! projects:
//!   - name: acme-corp
//!     path: ~/work/acme-secops
//!   - name: widgets-inc
//!     path: ~/work/widgets-secops
//! default: acme-corp
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors raised while reading the GUI's own config. These are user-facing
/// — surface them through the IPC layer with the YAML location intact so
/// the explainer card can point the operator at the file to fix.
#[derive(Debug, thiserror::Error)]
pub enum ProjectsError {
    #[error("could not locate ~/.config: HOME may be unset")]
    NoConfigDir,
    #[error("read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("parse state at {path}: {source}")]
    StateParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Raw shape of `projects.yaml` after deserialisation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectsConfig {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    /// Stored as the original (possibly `~`-prefixed) string for display
    /// purposes. Use [`ProjectEntry::resolved_path`] to get a real path.
    pub path: String,
}

impl ProjectEntry {
    /// Expand `~` against the supplied `home`; fall back to `dirs::home_dir()`
    /// when none is provided. Returns the original string verbatim if no
    /// expansion is required (preserves explicit absolute paths exactly).
    pub fn resolved_path_with_home(&self, home: Option<&Path>) -> PathBuf {
        if let Some(rest) = self.path.strip_prefix("~/") {
            if let Some(h) = home {
                return h.join(rest);
            }
            if let Some(h) = dirs::home_dir() {
                return h.join(rest);
            }
        } else if self.path == "~" {
            if let Some(h) = home {
                return h.to_path_buf();
            }
            if let Some(h) = dirs::home_dir() {
                return h;
            }
        }
        PathBuf::from(&self.path)
    }

    pub fn resolved_path(&self) -> PathBuf {
        self.resolved_path_with_home(None)
    }
}

/// View returned to the frontend: config + per-entry health flag plus the
/// previously-selected name (if any) so the UI can preselect on mount.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectsView {
    pub projects: Vec<ProjectEntryView>,
    pub default: Option<String>,
    pub last_selected: Option<String>,
    /// Source path the config was loaded from (always reported, even on
    /// missing-file, so the UI can show "create this file").
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntryView {
    pub name: String,
    pub path: String,
    pub resolved_path: String,
    /// `true` iff `resolved_path` exists on disk.
    pub exists: bool,
}

/// Persisted state — currently just the last-selected project name. The
/// frontend never reads this directly; the IPC layer surfaces it via
/// `ProjectsView`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedState {
    pub last_selected: Option<String>,
}

/// `~/.config/secunit-gui/` (or the platform equivalent for `dirs::config_dir`).
pub fn config_dir() -> Result<PathBuf, ProjectsError> {
    let base = dirs::config_dir().ok_or(ProjectsError::NoConfigDir)?;
    Ok(base.join("secunit-gui"))
}

pub fn projects_yaml_path() -> Result<PathBuf, ProjectsError> {
    Ok(config_dir()?.join("projects.yaml"))
}

pub fn state_json_path() -> Result<PathBuf, ProjectsError> {
    Ok(config_dir()?.join("state.json"))
}

/// Load `projects.yaml`. Missing file → empty config (not an error).
pub fn load_config(path: &Path) -> Result<ProjectsConfig, ProjectsError> {
    if !path.exists() {
        return Ok(ProjectsConfig::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| ProjectsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&text).map_err(|source| ProjectsError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub fn load_state(path: &Path) -> Result<PersistedState, ProjectsError> {
    if !path.exists() {
        return Ok(PersistedState::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| ProjectsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ProjectsError::StateParse {
        path: path.to_path_buf(),
        source,
    })
}

/// Persist the last-selected project. Atomic-rename: write to `*.tmp`,
/// fsync, rename. The state file lives in the GUI's own config dir and
/// never reaches inside a project tree.
pub fn save_state(path: &Path, state: &PersistedState) -> Result<(), ProjectsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ProjectsError::Read {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let body = serde_json::to_string_pretty(state).expect("PersistedState serialisable");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|source| ProjectsError::Read {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| ProjectsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Compose the `ProjectsView` returned to the frontend.
pub fn view_for(config: &ProjectsConfig, persisted: &PersistedState, source: &Path) -> ProjectsView {
    let projects = config
        .projects
        .iter()
        .map(|p| {
            let resolved = p.resolved_path();
            let exists = resolved.exists();
            ProjectEntryView {
                name: p.name.clone(),
                path: p.path.clone(),
                resolved_path: resolved.display().to_string(),
                exists,
            }
        })
        .collect();
    ProjectsView {
        projects,
        default: config.default.clone(),
        last_selected: persisted.last_selected.clone(),
        config_path: source.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_config_yields_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("projects.yaml");
        let cfg = load_config(&path).unwrap();
        assert!(cfg.projects.is_empty());
        assert!(cfg.default.is_none());
    }

    #[test]
    fn parses_two_projects() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("projects.yaml");
        fs::write(
            &path,
            "projects:\n  - name: a\n    path: /tmp/a\n  - name: b\n    path: ~/b\ndefault: a\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.projects.len(), 2);
        assert_eq!(cfg.default.as_deref(), Some("a"));
        assert_eq!(cfg.projects[0].path, "/tmp/a");
    }

    #[test]
    fn malformed_yaml_returns_parse_error_with_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("projects.yaml");
        fs::write(&path, "projects: [\n").unwrap();
        let err = load_config(&path).unwrap_err();
        match err {
            ProjectsError::Parse { path: p, .. } => assert_eq!(p, path),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn home_expansion() {
        let entry = ProjectEntry {
            name: "x".into(),
            path: "~/work/x".into(),
        };
        let home = std::path::PathBuf::from("/home/op");
        assert_eq!(
            entry.resolved_path_with_home(Some(&home)),
            home.join("work/x")
        );
    }

    #[test]
    fn home_expansion_bare_tilde() {
        let entry = ProjectEntry {
            name: "x".into(),
            path: "~".into(),
        };
        let home = std::path::PathBuf::from("/home/op");
        assert_eq!(entry.resolved_path_with_home(Some(&home)), home);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let s = PersistedState {
            last_selected: Some("widgets-inc".into()),
        };
        save_state(&path, &s).unwrap();
        let back = load_state(&path).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn view_marks_missing_paths_unhealthy() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real");
        fs::create_dir(&real).unwrap();
        let cfg = ProjectsConfig {
            projects: vec![
                ProjectEntry {
                    name: "real".into(),
                    path: real.display().to_string(),
                },
                ProjectEntry {
                    name: "missing".into(),
                    path: dir.path().join("ghost").display().to_string(),
                },
            ],
            default: Some("real".into()),
        };
        let v = view_for(&cfg, &PersistedState::default(), Path::new("/etc/x.yaml"));
        assert_eq!(v.projects.len(), 2);
        assert!(v.projects[0].exists);
        assert!(!v.projects[1].exists);
        assert_eq!(v.config_path, "/etc/x.yaml");
    }
}
