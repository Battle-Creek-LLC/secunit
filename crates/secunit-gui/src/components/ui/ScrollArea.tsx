import { forwardRef, type HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

/**
 * Thin styled scroller. Native scrollbars styled via CSS rather than the
 * Radix scroll-area component — fewer deps and the operator-tool feel
 * does not need custom thumbs.
 */
export const ScrollArea = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  function ScrollArea({ className, ...props }, ref) {
    return (
      <div
        ref={ref}
        className={cn(
          "h-full overflow-auto",
          "[scrollbar-width:thin]",
          "[&::-webkit-scrollbar]:h-2 [&::-webkit-scrollbar]:w-2",
          "[&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-border",
          className,
        )}
        {...props}
      />
    );
  },
);
