import { forwardRef, type HTMLAttributes, type TdHTMLAttributes, type ThHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

export const Table = forwardRef<HTMLTableElement, HTMLAttributes<HTMLTableElement>>(
  function Table({ className, ...props }, ref) {
    return (
      <div className="overflow-auto">
        <table
          ref={ref}
          className={cn("w-full caption-bottom border-collapse text-sm", className)}
          {...props}
        />
      </div>
    );
  },
);

export const THead = forwardRef<
  HTMLTableSectionElement,
  HTMLAttributes<HTMLTableSectionElement>
>(function THead({ className, ...props }, ref) {
  return (
    <thead
      ref={ref}
      className={cn(
        "sticky top-0 z-10 bg-background text-xs font-medium text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
});

export const TBody = forwardRef<HTMLTableSectionElement, HTMLAttributes<HTMLTableSectionElement>>(
  function TBody({ className, ...props }, ref) {
    return <tbody ref={ref} className={cn("divide-y", className)} {...props} />;
  },
);

export const TR = forwardRef<HTMLTableRowElement, HTMLAttributes<HTMLTableRowElement>>(
  function TR({ className, ...props }, ref) {
    return (
      <tr
        ref={ref}
        className={cn(
          "border-b transition-colors hover:bg-muted/40 data-[state=selected]:bg-muted",
          className,
        )}
        {...props}
      />
    );
  },
);

export const TH = forwardRef<HTMLTableCellElement, ThHTMLAttributes<HTMLTableCellElement>>(
  function TH({ className, ...props }, ref) {
    return (
      <th
        ref={ref}
        className={cn(
          "h-9 px-3 text-left align-middle font-medium text-muted-foreground border-b",
          className,
        )}
        {...props}
      />
    );
  },
);

export const TD = forwardRef<HTMLTableCellElement, TdHTMLAttributes<HTMLTableCellElement>>(
  function TD({ className, ...props }, ref) {
    return (
      <td
        ref={ref}
        className={cn("px-3 py-2 align-middle", className)}
        {...props}
      />
    );
  },
);
