import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Inject __APP_VERSION__ for components that read it at module scope.
// `vite.config.ts` does the equivalent via `define` for production builds;
// vitest runs without that pipeline.
(globalThis as unknown as { __APP_VERSION__: string }).__APP_VERSION__ =
  "0.0.0-test";

// Mock the Tauri IPC bridge so component tests run under jsdom without a
// running Tauri runtime. Tests override these per-suite.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {
    throw new Error(
      "@tauri-apps/api/core#invoke called without a per-suite mock",
    );
  }),
}));
