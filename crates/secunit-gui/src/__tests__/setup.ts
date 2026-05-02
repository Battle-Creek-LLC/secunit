import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

(globalThis as unknown as { __APP_VERSION__: string }).__APP_VERSION__ =
  "0.0.0-test";

// Mock the Tauri IPC bridge so component tests run under jsdom without
// a running Tauri runtime. Suites override `invoke` per-case.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {
    throw new Error(
      "@tauri-apps/api/core#invoke called without a per-suite mock",
    );
  }),
}));

// Watcher event bus is unused under jsdom but the App imports it on
// every render path. No-op listener that returns a no-op unsubscribe.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
  emit: vi.fn(async () => undefined),
}));
