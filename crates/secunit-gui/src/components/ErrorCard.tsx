interface ErrorCardProps {
  title: string;
  message: string;
}

export function ErrorCard({ title, message }: ErrorCardProps) {
  return (
    <main className="flex h-full flex-col items-center justify-center p-8">
      <div className="w-full max-w-xl rounded-lg border bg-background p-6">
        <h2 className="text-lg font-semibold text-error">{title}</h2>
        <pre className="mt-4 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs">
          <code>{message}</code>
        </pre>
      </div>
    </main>
  );
}
