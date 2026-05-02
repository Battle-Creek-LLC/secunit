import { useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui";
import { useStore } from "@/store";
import { AlertStrip } from "@/components/AlertStrip";
import { FocusList } from "@/components/FocusList";
import { HowAmIDoing } from "@/components/HowAmIDoing";
import { RunTimeline } from "@/components/RunTimeline";

const STALLED_DAYS = 3;
const DAY_MS = 24 * 60 * 60 * 1000;

export function Overview() {
  const snapshot = useStore();

  const counts = useMemo(() => {
    const now = Date.now();
    // Period coverage replaces the 7-day "due-soon" horizon. A control is
    // "open" when its current period exists and isn't yet satisfied;
    // "overdue" comes straight from the period status (a past gap, not a
    // grace-window edge). Continuous controls have no period and are
    // intentionally counted in neither bucket.
    let overdue = 0;
    let open = 0;
    snapshot.periods.forEach((p) => {
      if (p.status === "gap") overdue += 1;
      else if (p.status === "open") open += 1;
    });
    let stalled = 0;
    snapshot.runs.forEach((r) => {
      if (r.state !== "pending" || !r.started_at) return;
      const t = Date.parse(r.started_at);
      if (!Number.isNaN(t) && now - t > STALLED_DAYS * DAY_MS) stalled += 1;
    });
    return { overdue, dueSoon: open, stalled };
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
        </section>

        <section className="flex flex-col gap-3">
          <AlertStrip
            overdue={counts.overdue}
            dueSoon={counts.dueSoon}
            stalled={counts.stalled}
          />
          <div className="flex flex-col gap-2">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Focus now
            </h2>
            <FocusList
              controls={snapshot.controls}
              due={snapshot.due}
              periods={snapshot.periods}
              runs={snapshot.runs}
            />
          </div>
        </section>

        <section className="flex flex-col gap-2">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            How am I doing
          </h2>
          <HowAmIDoing controls={snapshot.controls} runs={snapshot.runs} />
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
