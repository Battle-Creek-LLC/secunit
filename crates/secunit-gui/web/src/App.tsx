export function App() {
  return (
    <main className="flex h-full flex-col items-center justify-center gap-2 p-8">
      <h1 className="text-2xl font-semibold tracking-tight">secunit</h1>
      <p className="text-sm text-muted-foreground">
        read-only viewer · v{__APP_VERSION__}
      </p>
    </main>
  );
}

declare const __APP_VERSION__: string;
