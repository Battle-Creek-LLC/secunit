# JOB-05 — App shell + design system (shadcn-style + Inter)

## Goal

The visual chrome the next six jobs hang views off: top bar, left nav, content pane, plus a small set of UI primitives modeled on shadcn.

## Deliverables

- `web/src/components/AppShell.tsx`:
  - Top bar (h-12, 1px bottom border, `bg-bg`, `text-fg`): project switcher (left), search trigger pill (centre, hint `⌘K`), help/about (right).
  - Left nav (w-56, 1px right border): six items — Overview, Controls, Schedule, Findings, Evidence, Inventory. Active item: subtle `bg-muted` + bold weight.
  - Content area: scroll container, max-width none, comfortable side padding.
- `web/src/components/ui/`:
  - `Button.tsx` (primary / ghost / outline / link).
  - `Card.tsx` (Card, CardHeader, CardTitle, CardContent, CardFooter).
  - `Badge.tsx` (variants: ok, warn, error, info, neutral).
  - `Table.tsx` (header + body, sticky header, zebra-free, dense option).
  - `Input.tsx`, `Label.tsx`.
  - `ScrollArea.tsx` (thin custom scrollbars).
  - `Tabs.tsx`, `Separator.tsx`, `Kbd.tsx`.
- `web/src/styles.css`:
  - CSS variables for the shadcn-style design tokens: `--background`, `--foreground`, `--muted`, `--muted-foreground`, `--border`, `--ring`, `--accent`, status hues (`--ok`, `--warn`, `--error`, `--info`). Light + dark variants under `prefers-color-scheme`.
  - Tailwind `@theme` directive uses the CSS vars so every primitive references the token, not the colour.
- React Router (or hash-based routing if Tauri 2's window can't accept `history.pushState`): six routes `/overview`, `/controls`, `/schedule`, `/findings`, `/evidence`, `/inventory`. Each renders a placeholder card for now.

## Non-goals

- No real data in any view — placeholders are fine. Views land in the next jobs.
- No keyboard shortcuts beyond the `⌘K` trigger appearing in the top bar (functional binding is JOB-12).

## Acceptance criteria

- The shell matches the spec layout and uses Inter for all UI text and JetBrains Mono (or system mono) for the project path shown in the switcher tooltip.
- Light and dark modes both render correctly; toggling `prefers-color-scheme` swaps tokens with no flash.
- Each nav item routes to its placeholder; the active route is highlighted.
- Visually compared against shadcn's reference shells: rounded-md cards, 1px borders, `--ring` focus state with a 2-pixel offset.

## Test plan

- **Frontend unit:** snapshot test for `AppShell` in both colour schemes; per-primitive Vitest tests asserting that variants apply expected classes.
- **A11y check:** every interactive element has a visible focus ring and `aria-current="page"` on the active nav link.
- **Manual smoke:** click through every nav item; confirm the placeholder cards appear; toggle OS colour scheme.

## Files touched

```
crates/secunit-gui/web/src/components/AppShell.tsx
crates/secunit-gui/web/src/components/ui/Button.tsx
crates/secunit-gui/web/src/components/ui/Card.tsx
crates/secunit-gui/web/src/components/ui/Badge.tsx
crates/secunit-gui/web/src/components/ui/Table.tsx
crates/secunit-gui/web/src/components/ui/Input.tsx
crates/secunit-gui/web/src/components/ui/Label.tsx
crates/secunit-gui/web/src/components/ui/ScrollArea.tsx
crates/secunit-gui/web/src/components/ui/Tabs.tsx
crates/secunit-gui/web/src/components/ui/Separator.tsx
crates/secunit-gui/web/src/components/ui/Kbd.tsx
crates/secunit-gui/web/src/styles.css
crates/secunit-gui/web/src/routes/*.tsx                 (six placeholders)
crates/secunit-gui/web/src/App.tsx
```
