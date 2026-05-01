import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui";

interface PlaceholderProps {
  title: string;
  description: string;
  job: string;
}

export function Placeholder({ title, description, job }: PlaceholderProps) {
  return (
    <div className="p-8">
      <div className="mx-auto max-w-3xl">
        <Card>
          <CardHeader>
            <CardTitle>{title}</CardTitle>
            <CardDescription>{description}</CardDescription>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              View lands in <span className="font-mono text-xs">{job}</span>.
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
