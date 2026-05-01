interface EmptyConfigProps {
  configPath: string;
}

const EXAMPLE = `projects:
  - name: acme-corp
    path: ~/work/acme-secops
  - name: widgets-inc
    path: ~/work/widgets-secops
default: acme-corp
`;

export function EmptyConfig({ configPath }: EmptyConfigProps) {
  return (
    <main className="flex h-full flex-col items-center justify-center p-8">
      <div className="w-full max-w-xl rounded-lg border bg-background p-6">
        <h2 className="text-lg font-semibold">No projects configured</h2>
        <p className="mt-2 text-sm text-muted-foreground">
          Create a YAML file at:
        </p>
        <pre className="mt-2 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs">
          <code>{configPath}</code>
        </pre>
        <p className="mt-4 text-sm text-muted-foreground">Example:</p>
        <pre className="mt-2 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs">
          <code>{EXAMPLE}</code>
        </pre>
        <p className="mt-4 text-xs text-muted-foreground">
          The viewer reloads the file on each launch.
        </p>
      </div>
    </main>
  );
}
