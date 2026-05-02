import type { Config } from "tailwindcss";

// shadcn-style design tokens. Colours are CSS variables defined in
// `src/styles.css` so dark/light variants come for free via
// `prefers-color-scheme`.
const config: Config = {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: [
          "Inter",
          "ui-sans-serif",
          "system-ui",
          "-apple-system",
          "sans-serif",
        ],
        mono: [
          "JetBrains Mono",
          "ui-monospace",
          "SFMono-Regular",
          "Menlo",
          "monospace",
        ],
      },
      colors: {
        border: "hsl(var(--border))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        ok: "hsl(var(--ok))",
        warn: "hsl(var(--warn))",
        error: "hsl(var(--error))",
        info: "hsl(var(--info))",
      },
      borderRadius: {
        md: "calc(var(--radius) - 2px)",
        lg: "var(--radius)",
        sm: "calc(var(--radius) - 4px)",
      },
      fontSize: {
        xs: ["0.75rem", { lineHeight: "1rem" }],
        sm: ["0.8125rem", { lineHeight: "1.125rem" }],
        base: ["0.875rem", { lineHeight: "1.25rem" }],
      },
    },
  },
  plugins: [],
};

export default config;
