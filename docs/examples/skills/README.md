# Example skills

The reusable runbooks — `capture-sweep`, `attestation-review`,
`policy-annual-review`, `report`, `bootstrap`, `inventory-seed` — are **bundled
into the `secunit` binary**, not duplicated here. Read them with:

```
secunit skills list
secunit skills show <name>
```

This directory keeps one illustrative example, `sca-weekly-dependency-scan.md`,
showing the shape of a **bespoke per-control skill** — the local-file escape hatch
for a control whose logic fits no bundled runbook. Most controls don't need this;
they name a bundled runbook and pass specifics through `skill_args`. See
`docs/skills.md` for the resolution model (local override → bundled) and the
spine/fragment (`skill_args.extend`) composition pattern.
