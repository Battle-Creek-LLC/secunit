import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle, type BadgeVariant } from "@/components/ui";
import { useStore } from "@/store";
import { HealthTile } from "@/components/HealthTile";
import { RunTimeline } from "@/components/RunTimeline";

export function Overview() {
  const snapshot = useStore();

  const tiles = useMemo(() => {
    const now = Date.now();
    const cutoff7d = now + 7 * 24 * 60 * 60 * 1000;
    const cutoff30dPast = now - 30 * 24 * 60 * 60 * 1000;

    let overdue = 0;
    snapshot.controls.forEach((c) => {
      if (c.status === "overdue") overdue += 1;
    });

    let dueSoon = 0;
    snapshot.due.forEach((d) => {
      if (d.overdue || !d.next_due) return;
      const t = Date.parse(d.next_due);
      if (!Number.isNaN(t) && t >= now && t <= cutoff7d) dueSoon += 1;
    });

    const inProgress = snapshot.runs.filter((r) => r.state === "pending").length;
    const sealedRecent = snapshot.runs.filter(
      (r) =>
        r.state === "sealed" &&
        r.completed_at !== null &&
        Date.parse(r.completed_at) >= cutoff30dPast,
    ).length;

    return [
      {
        label: "Overdue",
        value: overdue,
        caption: "past grace window",
        to: "/controls?status=overdue",
        tone: (overdue > 0 ? "error" : "neutral") as BadgeVariant,
      },
      {
        label: "Due this week",
        value: dueSoon,
        caption: "next 7 days",
        to: "/schedule",
        tone: (dueSoon > 0 ? "warn" : "neutral") as BadgeVariant,
      },
      {
        label: "In progress",
        value: inProgress,
        caption: "prepared but not sealed",
        to: "/evidence",
        tone: (inProgress > 0 ? "info" : "neutral") as BadgeVariant,
      },
      {
        label: "Sealed last 30d",
        value: sealedRecent,
        caption: "manifests written",
        to: "/evidence",
        tone: "ok" as const,
      },
    ];
  }, [snapshot]);

  const recent = useMemo(() => snapshot.runs.slice(0, 25), [snapshot.runs]);

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="mx-auto flex max-w-5xl flex-col gap-6">
        <section>
          <h1 className="text-base font-semibold">Overview</h1>
          <p className="text-xs text-muted-foreground">
            Live counts and the latest runs across the project. Tiles link to
            their detail view.
          </p>
        </section>
        <section className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {tiles.map((t) => (
            <HealthTile key={t.label} {...t} />
          ))}
        </section>
        <Card>
          <CardHeader>
            <CardTitle>Recent runs</CardTitle>
          </CardHeader>
          <CardContent className="p-0">
            <RunTimeline runs={recent} emptyHint="no runs in evidence/ yet" />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
