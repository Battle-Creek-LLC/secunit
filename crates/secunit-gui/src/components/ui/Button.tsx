import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/cn";

type Variant = "primary" | "ghost" | "outline" | "link";
type Size = "default" | "sm" | "icon";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

const variants: Record<Variant, string> = {
  primary:
    "bg-foreground text-background hover:opacity-90 focus-visible:ring-ring",
  ghost: "hover:bg-muted text-foreground focus-visible:ring-ring",
  outline:
    "border bg-background text-foreground hover:bg-muted focus-visible:ring-ring",
  link: "text-foreground underline-offset-4 hover:underline focus-visible:ring-ring",
};

const sizes: Record<Size, string> = {
  default: "h-8 px-3 text-sm",
  sm: "h-7 px-2 text-xs",
  icon: "h-8 w-8 p-0",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant = "outline", size = "default", ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-md font-medium transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        "disabled:pointer-events-none disabled:opacity-50",
        variants[variant],
        sizes[size],
        className,
      )}
      {...props}
    />
  );
});
