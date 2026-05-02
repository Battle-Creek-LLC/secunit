#!/usr/bin/env bash
# Cleanup helper for secunit projects that predate the drop-aborted-state
# refactor. The new code ignores abort.json entirely; this script removes
# orphaned abort.json sidecars so aborted runs don't linger as confusing
# "phantom pending" entries in the GUI.
#
# It deliberately does NOT synthesize manifests for the aborted runs.
# That would require rewriting prior_run links on every later sealed
# manifest (cascading sha changes), which invalidates the cryptographic
# chain integrity. Operators who want to "close out" an aborted run can
# instead run `secunit run abort <run_dir>` against it under the new
# code, which seals a failed manifest with the current registry state.
#
# Usage: scripts/migrate-drop-abort.sh <secunit-project-root>

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <secunit-project-root>" >&2
  exit 2
fi

root="$1"
if [[ ! -d "$root/evidence" ]]; then
  echo "error: $root/evidence does not exist" >&2
  exit 1
fi

aborts=()
while IFS= read -r line; do
  aborts+=("$line")
done < <(find "$root/evidence" -type f -name abort.json 2>/dev/null)

if [[ ${#aborts[@]} -eq 0 ]]; then
  echo "no abort.json files found under $root/evidence — nothing to do"
  exit 0
fi

echo "found ${#aborts[@]} abort.json file(s) to remove:"
printf '  %s\n' "${aborts[@]}"
echo

read -r -p "remove these files? [y/N] " ans
if [[ ! "$ans" =~ ^[Yy]$ ]]; then
  echo "aborted; nothing changed"
  exit 0
fi

for f in "${aborts[@]}"; do
  rm -- "$f"
  echo "removed $f"
done

echo "done — ${#aborts[@]} file(s) removed"
