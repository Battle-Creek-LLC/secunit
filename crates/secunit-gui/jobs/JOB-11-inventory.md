# JOB-11 — Inventory view

## Goal

A read-only table of `inventory.yaml`, grouped by kind, with active/retired badges per entry.

## Deliverables

- `web/src/routes/Inventory.tsx`:
  - Section per kind (`source_repos`, `cloud_accounts`, `saas`, `endpoints`, `sites`, ...). Header shows kind + active count / total.
  - Table per section: name, tags (chips), in-scope-since, retired-on (if any), active-today badge, extras (URL, stack, provider, owner — only the keys present, sorted).
  - A search box filters across name, tags, extras values.
  - "Open inventory.yaml" button (top right) calls `open_in_editor` on `<root>/inventory.yaml`.

## Non-goals

- No editing. The CLI / git remain the only paths.
- No multi-org merge. One project at a time.

## Acceptance criteria

- The table matches `inventory.yaml` byte-for-byte in content (no ordering differences within a kind beyond the documented sort).
- A search for a tag produces hits across multiple kinds.
- A future-dated `in_scope_since` shows a `not-yet` badge instead of `active`; a past `retired_on` shows a `retired` badge.
- Live update: editing `inventory.yaml` updates the table within the debounce window.

## Test plan

- **Frontend unit:** sectioning, search, badge variants for active/retired/not-yet.
- **Manual smoke:** review the fixture's inventory; counts in section headers match a manual count.

## Files touched

```
crates/secunit-gui/web/src/routes/Inventory.tsx
crates/secunit-gui/web/src/components/InventorySection.tsx
crates/secunit-gui/web/src/components/InventoryRow.tsx
crates/secunit-gui/web/src/__tests__/Inventory.test.tsx
```
