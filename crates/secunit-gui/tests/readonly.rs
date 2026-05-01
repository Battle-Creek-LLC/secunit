//! Read-only contract audit.
//!
//! Every IPC command exposed to the webview must be on the allow-list
//! below. Adding a new command means adding it to this list **and**
//! to the JOB-13 review checklist in the PR description.
//!
//! What this test catches:
//!   * a `pub fn` annotated `#[tauri::command]` is added under
//!     `src/api/` without being added to the allow-list — fails with a
//!     diff so the reviewer sees what's new.
//!   * an entry on the allow-list points at a function that no longer
//!     exists — fails for the symmetric reason; the list does not rot.
//!
//! What it does NOT catch: a command on the list that LOOKS read-only
//! by name but actually mutates. That's a code-review job. The
//! allow-list is the gate that focuses the review.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const ALLOWLIST: &[&str] = &[
    // Project config (JOB-02)
    "list_projects",
    "select_project",
    "current_project",
    // Registry (JOB-03)
    "load_project",
    "list_controls",
    "get_control",
    "due_rows",
    "get_inventory",
    "list_runs",
    "recent_runs",
    "get_run",
    // Findings (JOB-09)
    "list_findings",
    "read_findings",
    // Evidence preview (JOB-10)
    "read_artifact",
    // Schedule (JOB-08)
    "schedule_view",
    // Search (JOB-12)
    "search_palette",
    "index_status",
];

fn api_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/api")
}

/// Collect the set of `pub fn <name>` defined directly above a
/// `#[tauri::command]` attribute under `src/api/`.
fn discovered_commands() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in fs::read_dir(api_dir()).expect("read src/api") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read api file");
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim().starts_with("#[tauri::command]") {
                // Skip past any further attributes.
                while let Some(peek) = lines.peek() {
                    if peek.trim_start().starts_with("#[") {
                        lines.next();
                    } else {
                        break;
                    }
                }
                if let Some(sig) = lines.next() {
                    if let Some(name) = parse_pub_fn_name(sig) {
                        out.insert(name);
                    }
                }
            }
        }
    }
    out
}

/// Pull the `name` out of `pub fn name(...)`.
fn parse_pub_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after = trimmed.strip_prefix("pub fn ")?;
    let end = after.find(['(', '<', ' ']).unwrap_or(after.len());
    Some(after[..end].to_string())
}

#[test]
fn allowlist_matches_registered_commands() {
    let expected: BTreeSet<String> = ALLOWLIST.iter().map(|s| s.to_string()).collect();
    let discovered = discovered_commands();

    let only_on_disk: Vec<_> = discovered.difference(&expected).cloned().collect();
    let only_on_list: Vec<_> = expected.difference(&discovered).cloned().collect();

    assert!(
        only_on_disk.is_empty() && only_on_list.is_empty(),
        "read-only allow-list drifted from the IPC surface.\n\
         New commands found in src/api/ but missing from ALLOWLIST: {only_on_disk:?}\n\
         Stale entries in ALLOWLIST that no longer exist:           {only_on_list:?}\n\
         If the new commands are read-only, add them to ALLOWLIST in\n\
         tests/readonly.rs and review them against the JOB-13 checklist.\n\
         If a command was removed, drop it from ALLOWLIST in the same commit."
    );
}

#[test]
fn no_command_name_smells_like_a_write() {
    // Pure-name heuristic. Catches obvious slips during review even
    // when the implementation is in a separate file. Update the deny
    // list as new vocabulary surfaces.
    const DENY: &[&str] = &[
        "write_", "create_", "delete_", "remove_", "edit_", "save_", "set_", "update_",
        "patch_", "mutate_", "commit_", "push_",
    ];
    for name in ALLOWLIST {
        for bad in DENY {
            assert!(
                !name.starts_with(bad),
                "command `{name}` starts with a write-shaped prefix `{bad}`. \
                 Either the command is a write (which JOB-13 forbids) or it \
                 needs renaming to make the read-only contract obvious."
            );
        }
    }
}

#[test]
fn capabilities_grant_no_fs_write() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let body = fs::read_to_string(&path).expect("read capabilities/default.json");
    // Spot-check the obvious banned permissions. Tauri 2's permission
    // names follow `<plugin>:<verb>` for plugin-defined permissions and
    // `core:<verb>` for built-ins. None of the strings below should
    // appear in a read-only viewer's capability set.
    for banned in &[
        "fs:write",
        "fs:create",
        "fs:remove",
        "fs:rename",
        "fs:write-text-file",
        "fs:write-binary-file",
        "fs:allow-write",
    ] {
        assert!(
            !body.contains(banned),
            "capabilities/default.json contains banned permission `{banned}` — \
             the GUI must not be granted any write-shaped fs permission. See JOB-13."
        );
    }
}
