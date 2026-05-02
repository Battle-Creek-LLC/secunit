import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

export function Kbd({ className, ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 select-none items-center rounded-sm border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}
