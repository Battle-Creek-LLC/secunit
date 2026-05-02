# JOB-10 — Evidence view

## Goal

A file browser mirroring the on-disk layout of `evidence/<y>/<q>/<control>/<run>/`, with run-state badges and artifact preview for safe file types.

## Deliverables

- `web/src/routes/Evidence.tsx`:
  - Three-pane layout: tree (left), run summary (centre), artifact preview (right).
  - Tree: lazy-expanded — Year → Quarter → Control → Run. Run nodes show a status badge (sealed / aborted / pending) and a relative timestamp.
  - Run summary: title (control + run id), prepared / completed timestamps, agent block (skill, model, hashes — mono), prior run link (if any), `by_system` summary table, top-level artifacts table.
  - Artifact preview:
    - `.md` → rendered (reuse the JOB-09 sanitiser).
    - `.json` → pretty-printed with a tree toggle (collapsed by default beyond depth 3).
    - `.txt` / `.log` / `.yaml` → mono code view, monospace, syntax-tagged where trivial (line numbers, no Prism).
    - Image (`.png`, `.jpg`, `.svg`) → bounded preview.
    - Otherwise → metadata only (size, sha256, "open in editor", "reveal in finder").
  - Cap previews at 2 MiB; larger files show metadata + the open/reveal buttons only.
- IPC additions: `list_run_tree(control_id, run_id) -> RunTree` returning the directory + file structure with sizes and hashes from the manifest where present.

## Non-goals

- No diff between two runs. Future.
- No download / export buttons (would breach read-only feel).

## Acceptance criteria

- Browsing the fixture's `evidence/2026/q2/...` produces a tree exactly mirroring the on-disk layout.
- Sealed manifests show their `manifest_sha256` in the run summary; pending runs show a yellow `pending` badge and no manifest.
- Selecting a `findings.md` artifact renders identically to the same file in JOB-09.
- A path-traversal probe (`../../etc/passwd`) returns an error from `read_artifact`, not data.
- Live update: a new artifact appearing inside an open run dir surfaces in the tree.

## Test plan

- **Rust unit:** `list_run_tree` against the fixture; assert depth, ordering, manifest-hash present where expected.
- **Frontend unit:** tree expansion, preview-type routing, size cap behaviour with a synthetic 3 MiB file.
- **Manual smoke:** browse a sealed run, an aborted run, a pending run; confirm badges.

## Files touched

```
crates/secunit-gui/src-tauri/src/api.rs                 (list_run_tree)
crates/secunit-gui/web/src/routes/Evidence.tsx
crates/secunit-gui/web/src/components/RunTree.tsx
crates/secunit-gui/web/src/components/RunSummary.tsx
crates/secunit-gui/web/src/components/ArtifactPreview.tsx
crates/secunit-gui/web/src/__tests__/Evidence.test.tsx
```
