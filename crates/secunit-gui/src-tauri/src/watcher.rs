//! Single-instance, debounced filesystem watcher per open project.
//!
//! Translates raw `notify` events into typed payloads the webview cares
//! about (`control_changed`, `inventory_changed`, `state_json_changed`,
//! `run_state_changed`, `findings_changed`) and emits them via a generic
//! [`EventSink`] so unit tests can drive the same code without a Tauri
//! runtime.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use secunit_core::model::State;
use serde::Serialize;

use crate::state::AppState;

const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Top-level event emitted to the webview. The frontend reacts by
/// re-fetching the affected slice through the IPC commands from JOB-03.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WatcherEvent {
    ControlChanged {
        id: String,
        path: String,
    },
    InventoryChanged {
        path: String,
    },
    StateJsonChanged {
        path: String,
    },
    RunStateChanged {
        control_id: String,
        run_id: String,
        change: RunChange,
    },
    FindingsChanged {
        control_id: String,
        run_id: String,
        path: String,
    },
    /// Anything under `risks/` changed — a new event line appended to a
    /// risk log, or the derived `index.json` refreshed. The webview
    /// re-reads the register (and any open risk detail). `risk_id` is the
    /// `R-NNNN` directory when the change is scoped to one log; `None`
    /// for `index.json` itself.
    RisksChanged {
        risk_id: Option<String>,
        path: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunChange {
    Sealed,
    PreparedOrModified,
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: WatcherEvent);
}

/// Tauri-backed sink. Each event becomes a webview event named after
/// `WatcherEvent`'s tag (`control_changed`, …).
pub struct TauriSink {
    pub handle: tauri::AppHandle,
}

impl EventSink for TauriSink {
    fn emit(&self, event: WatcherEvent) {
        use tauri::{Emitter, Manager};

        // Refresh the in-memory cache *before* the webview is told to
        // re-fetch — otherwise list_controls would return pre-change
        // data (e.g. last_status="in-progress" after finalize already
        // wrote "complete" to state.json).
        if let WatcherEvent::StateJsonChanged { path } = &event {
            let app_state = self.handle.state::<AppState>();
            refresh_state_cache(&app_state, Path::new(path));
        }

        let topic = match &event {
            WatcherEvent::ControlChanged { .. } => "control_changed",
            WatcherEvent::InventoryChanged { .. } => "inventory_changed",
            WatcherEvent::StateJsonChanged { .. } => "state_json_changed",
            WatcherEvent::RunStateChanged { .. } => "run_state_changed",
            WatcherEvent::FindingsChanged { .. } => "findings_changed",
            WatcherEvent::RisksChanged { .. } => "risks_changed",
        };
        if let Err(err) = self.handle.emit(topic, event) {
            tracing::warn!(error = %err, "failed to emit watcher event");
        }
    }
}

/// Re-read `state.json` and swap it into the cached `LoadedProject`. A read
/// or parse error logs a warning and leaves the cache untouched — better to
/// serve slightly-stale data than to wipe a working cache because of a
/// transient half-written file (atomic_write happens *very* near the
/// debounce boundary).
pub(crate) fn refresh_state_cache(app_state: &AppState, path: &Path) {
    let parsed = match read_state(path) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(error = %err, path = %path.display(), "refresh state.json cache");
            return;
        }
    };
    let mut slot = app_state.project.lock().expect("AppState.project poisoned");
    if let Some(loaded) = slot.as_mut() {
        loaded.registry.state = parsed;
    }
}

fn read_state(path: &Path) -> Result<State, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))
}

/// Owns the debouncer and the drain thread. Dropping it stops the watch.
pub struct WatcherHandle {
    // Held to keep watching alive; never accessed by name.
    _debouncer: notify_debouncer_full::Debouncer<
        notify::RecommendedWatcher,
        notify_debouncer_full::RecommendedCache,
    >,
    join: Option<thread::JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Start watching `root`. The returned handle keeps the watch alive; drop
/// it to stop. `debounce_ms` falls back to `DEFAULT_DEBOUNCE_MS` if 0.
pub fn start<S: EventSink>(
    root: &Path,
    sink: S,
    debounce_ms: u64,
) -> Result<WatcherHandle, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("canonicalise {}: {e}", root.display()))?;
    let debounce = Duration::from_millis(if debounce_ms == 0 {
        DEFAULT_DEBOUNCE_MS
    } else {
        debounce_ms
    });

    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(debounce, None, tx).map_err(|e| format!("debouncer: {e}"))?;
    debouncer
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| format!("watch {}: {e}", root.display()))?;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let drain_stop = Arc::clone(&stop);
    let drain_root = root.clone();
    let join = thread::Builder::new()
        .name("secunit-gui-watch".into())
        .spawn(move || drain_loop(rx, sink, &drain_root, drain_stop))
        .map_err(|e| format!("spawn watcher thread: {e}"))?;

    tracing::info!(root = %root.display(), debounce_ms = debounce.as_millis() as u64, "watcher started");
    Ok(WatcherHandle {
        _debouncer: debouncer,
        join: Some(join),
        stop,
    })
}

fn drain_loop<S: EventSink>(
    rx: std::sync::mpsc::Receiver<DebounceEventResult>,
    sink: S,
    root: &Path,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    loop {
        if stop.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
        let batch = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(b) => b,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let events = match batch {
            Ok(events) => events,
            Err(errs) => {
                for err in errs {
                    tracing::warn!(error = ?err, "watcher error");
                }
                continue;
            }
        };
        // Deduplicate: a debounce window can carry multiple events per
        // path (create + modify on first write). Keep the last by path.
        let mut seen = std::collections::BTreeSet::<PathBuf>::new();
        for de in events {
            for raw in de.event.paths.iter() {
                let path = raw.clone();
                if !seen.insert(path.clone()) {
                    continue;
                }
                if let Some(ev) = classify(root, &path) {
                    sink.emit(ev);
                }
            }
        }
    }
}

/// Map an absolute path inside `root` to a typed [`WatcherEvent`].
/// Returns `None` for paths the GUI does not care about.
fn classify(root: &Path, path: &Path) -> Option<WatcherEvent> {
    let rel = path.strip_prefix(root).ok()?;
    let components: Vec<&str> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    match components.as_slice() {
        ["controls", file] => {
            let id = file.strip_suffix(".yaml")?.to_string();
            Some(WatcherEvent::ControlChanged {
                id,
                path: path.display().to_string(),
            })
        }
        ["inventory.yaml"] => Some(WatcherEvent::InventoryChanged {
            path: path.display().to_string(),
        }),
        ["state.json"] => Some(WatcherEvent::StateJsonChanged {
            path: path.display().to_string(),
        }),
        // evidence/<y>/<q>/<control>/<run>/<...>
        ["evidence", _y, _q, control, run, rest @ ..] => {
            classify_evidence(control, run, rest, path)
        }
        // risks/index.json — the derived register cache refreshed.
        ["risks", "index.json"] => Some(WatcherEvent::RisksChanged {
            risk_id: None,
            path: path.display().to_string(),
        }),
        // risks/<R-NNNN>/events.jsonl — a log appended to.
        ["risks", risk_id, "events.jsonl"] => Some(WatcherEvent::RisksChanged {
            risk_id: Some((*risk_id).to_string()),
            path: path.display().to_string(),
        }),
        _ => None,
    }
}

fn classify_evidence(control: &str, run: &str, rest: &[&str], abs: &Path) -> Option<WatcherEvent> {
    if rest.is_empty() {
        return None;
    }
    match *rest.last().unwrap() {
        "manifest.json" => Some(WatcherEvent::RunStateChanged {
            control_id: control.into(),
            run_id: run.into(),
            change: RunChange::Sealed,
        }),
        ".run-pending" | "prepare.json" => Some(WatcherEvent::RunStateChanged {
            control_id: control.into(),
            run_id: run.into(),
            change: RunChange::PreparedOrModified,
        }),
        "findings.md" => Some(WatcherEvent::FindingsChanged {
            control_id: control.into(),
            run_id: run.into(),
            path: abs.display().to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default, Clone)]
    struct CountingSink {
        events: Arc<Mutex<Vec<WatcherEvent>>>,
    }

    impl EventSink for CountingSink {
        fn emit(&self, event: WatcherEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn settle(sink: &CountingSink, predicate: impl Fn(&[WatcherEvent]) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            {
                let g = sink.events.lock().unwrap();
                if predicate(&g) {
                    return;
                }
            }
            if std::time::Instant::now() >= deadline {
                let g = sink.events.lock().unwrap();
                panic!("timed out; events so far: {:#?}", *g);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn classify_known_paths() {
        let root = Path::new("/r");
        assert_eq!(
            classify(root, &root.join("controls/foo.yaml")),
            Some(WatcherEvent::ControlChanged {
                id: "foo".into(),
                path: "/r/controls/foo.yaml".into(),
            })
        );
        assert_eq!(
            classify(root, &root.join("inventory.yaml")),
            Some(WatcherEvent::InventoryChanged {
                path: "/r/inventory.yaml".into(),
            })
        );
        assert_eq!(
            classify(root, &root.join("state.json")),
            Some(WatcherEvent::StateJsonChanged {
                path: "/r/state.json".into(),
            })
        );
        assert_eq!(
            classify(
                root,
                &root.join("evidence/2026/q2/aa-weekly-audit-review/run-001/manifest.json")
            ),
            Some(WatcherEvent::RunStateChanged {
                control_id: "aa-weekly-audit-review".into(),
                run_id: "run-001".into(),
                change: RunChange::Sealed,
            })
        );
        assert_eq!(
            classify(
                root,
                &root.join(
                    "evidence/2026/q2/aa-weekly-audit-review/run-001/by-system/x/findings.md"
                )
            ),
            Some(WatcherEvent::FindingsChanged {
                control_id: "aa-weekly-audit-review".into(),
                run_id: "run-001".into(),
                path: "/r/evidence/2026/q2/aa-weekly-audit-review/run-001/by-system/x/findings.md"
                    .into(),
            })
        );
        assert_eq!(
            classify(root, &root.join("risks/index.json")),
            Some(WatcherEvent::RisksChanged {
                risk_id: None,
                path: "/r/risks/index.json".into(),
            })
        );
        assert_eq!(
            classify(root, &root.join("risks/R-0007/events.jsonl")),
            Some(WatcherEvent::RisksChanged {
                risk_id: Some("R-0007".into()),
                path: "/r/risks/R-0007/events.jsonl".into(),
            })
        );
        assert_eq!(classify(root, &root.join("README.md")), None);
        assert_eq!(classify(root, &root.join("controls/foo.txt")), None);
        assert_eq!(classify(root, &root.join("risks/R-0007/notes.txt")), None);
    }

    #[test]
    fn integration_emits_control_changed_within_debounce() {
        let dir = tempfile::tempdir().unwrap();
        let controls = dir.path().join("controls");
        std::fs::create_dir(&controls).unwrap();
        let target = controls.join("aa-weekly-audit-review.yaml");
        std::fs::write(&target, "id: aa-weekly-audit-review\n").unwrap();

        let sink = CountingSink::default();
        let _handle = start(dir.path(), sink.clone(), 100).unwrap();

        // Touch the file after the watcher is up.
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(&target, "id: aa-weekly-audit-review\nupdated: true\n").unwrap();

        settle(&sink, |evs| {
            evs.iter().any(|e| {
                matches!(
                    e,
                    WatcherEvent::ControlChanged { id, .. } if id == "aa-weekly-audit-review"
                )
            })
        });
    }

    #[test]
    fn integration_coalesces_burst_to_one_per_path() {
        let dir = tempfile::tempdir().unwrap();
        let controls = dir.path().join("controls");
        std::fs::create_dir(&controls).unwrap();
        let target = controls.join("ac.yaml");
        std::fs::write(&target, "v: 0\n").unwrap();

        let sink = CountingSink::default();
        let _handle = start(dir.path(), sink.clone(), 150).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        for i in 1..=10 {
            std::fs::write(&target, format!("v: {i}\n")).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }

        // Allow the debounce window to flush.
        std::thread::sleep(Duration::from_millis(400));

        let events = sink.events.lock().unwrap().clone();
        let count = events
            .iter()
            .filter(|e| matches!(e, WatcherEvent::ControlChanged { id, .. } if id == "ac"))
            .count();
        // The point: 10 writes do not produce 10 events. The exact
        // collapse depends on how notify's batches line up with the
        // debounce window on this OS — accept anything well below 10.
        assert!(
            (1..=4).contains(&count),
            "expected ≤4 coalesced events from a 10-write burst, saw {count}: {events:#?}"
        );
    }

    #[test]
    fn refresh_state_cache_swaps_in_new_state_json() {
        use crate::state::{AppState, LoadedProject};
        use secunit_core::registry::loader;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/orgs/multi-system");

        let (registry, _) = loader::load(&fixture);
        // Sanity-check the fixture so a later edit doesn't silently
        // invalidate this test.
        let initial = registry
            .state
            .controls
            .get("aa-weekly-audit-review")
            .expect("fixture has aa-weekly-audit-review");
        assert_eq!(initial.last_run_id.as_deref(), Some("2026-04-26-run-013"));

        let app_state = AppState::default();
        *app_state.project.lock().unwrap() = Some(LoadedProject {
            name: "test".into(),
            root: fixture.clone(),
            registry,
            diagnostics: vec![],
        });

        // Write a fresh state.json in a *different* root so we don't
        // perturb the on-disk fixture; refresh_state_cache reads from
        // the path we hand it, not from the project root.
        let dir = tempfile::tempdir().unwrap();
        let new_state = dir.path().join("state.json");
        std::fs::write(
            &new_state,
            r#"{
              "schema_version": 1,
              "controls": {
                "aa-weekly-audit-review": {
                  "last_run_id": "2026-05-02-run-003",
                  "last_run_path": "evidence/2026/q2/aa-weekly-audit-review/2026-05-02-run-003/",
                  "last_run_at": "2026-05-02T13:58:01Z",
                  "last_status": "complete",
                  "next_due": "2026-05-04"
                }
              },
              "updated_at": "2026-05-02T13:58:02Z"
            }"#,
        )
        .unwrap();

        refresh_state_cache(&app_state, &new_state);

        let slot = app_state.project.lock().unwrap();
        let entry = slot
            .as_ref()
            .unwrap()
            .registry
            .state
            .controls
            .get("aa-weekly-audit-review")
            .unwrap();
        assert_eq!(entry.last_run_id.as_deref(), Some("2026-05-02-run-003"));
    }

    #[test]
    fn refresh_state_cache_keeps_old_data_on_corrupt_file() {
        use crate::state::{AppState, LoadedProject};
        use secunit_core::registry::loader;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/orgs/multi-system");
        let (registry, _) = loader::load(&fixture);
        let app_state = AppState::default();
        *app_state.project.lock().unwrap() = Some(LoadedProject {
            name: "test".into(),
            root: fixture.clone(),
            registry,
            diagnostics: vec![],
        });

        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("state.json");
        std::fs::write(&bad, "{ not json").unwrap();

        refresh_state_cache(&app_state, &bad);

        let slot = app_state.project.lock().unwrap();
        let entry = slot
            .as_ref()
            .unwrap()
            .registry
            .state
            .controls
            .get("aa-weekly-audit-review")
            .unwrap();
        // Untouched: the parse error logged but we kept what we had.
        assert_eq!(entry.last_run_id.as_deref(), Some("2026-04-26-run-013"));
    }

    #[test]
    fn integration_new_manifest_emits_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir
            .path()
            .join("evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001");
        std::fs::create_dir_all(&run).unwrap();

        let sink = CountingSink::default();
        let _handle = start(dir.path(), sink.clone(), 100).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        std::fs::write(run.join("manifest.json"), "{}").unwrap();

        settle(&sink, |evs| {
            evs.iter().any(|e| {
                matches!(
                    e,
                    WatcherEvent::RunStateChanged { change, control_id, .. }
                        if *change == RunChange::Sealed
                        && control_id == "sca-weekly-dependency-scan"
                )
            })
        });
    }
}
