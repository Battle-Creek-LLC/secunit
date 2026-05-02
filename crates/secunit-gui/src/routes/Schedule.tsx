import { useEffect, useState } from "react";
import {
  Card,
  CardContent,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui";
import { scheduleView, type ScheduleEntryView } from "@/lib/ipc";
import { useStore } from "@/store";
import { ScheduleList } from "@/components/ScheduleList";
import { Calendar } from "@/components/Calendar";

export function Schedule() {
  const snapshot = useStore();
  const [entries, setEntries] = useState<ScheduleEntryView[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (snapshot.revision === 0) return;
    let cancelled = false;
    scheduleView()
      .then((rows) => {
        if (!cancelled) setEntries(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [snapshot.revision]);

  if (snapshot.revision === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        loading project…
      </div>
    );
  }
  if (error) {
    return (
      <div className="p-6 text-sm text-error">Failed to load schedule: {error}</div>
    );
  }

  const overdueCount = entries.filter((e) => e.overdue).length;
  const total = entries.length;

  return (
    <div className="p-6">
      <div className="mx-auto flex max-w-6xl flex-col gap-4">
        <header className="flex items-end justify-between">
          <div>
            <h1 className="text-base font-semibold">Schedule</h1>
            <p className="text-xs text-muted-foreground">
              {total} upcoming firing{total === 1 ? "" : "s"}
              {overdueCount > 0 && ` · ${overdueCount} overdue`}
            </p>
          </div>
        </header>

        <Tabs defaultValue="list">
          <TabsList>
            <TabsTrigger value="calendar">Calendar</TabsTrigger>
            <TabsTrigger value="list">List</TabsTrigger>
          </TabsList>
          <TabsContent value="calendar">
            <Card>
              <CardContent className="p-0">
                <Calendar entries={entries} />
              </CardContent>
            </Card>
          </TabsContent>
          <TabsContent value="list">
            <ScheduleList entries={entries} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}
