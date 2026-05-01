import "@testing-library/jest-dom/vitest";

// Inject __APP_VERSION__ for components that read it at module scope.
// `vite.config.ts` does the equivalent via `define` for production builds;
// vitest runs without that pipeline.
(globalThis as unknown as { __APP_VERSION__: string }).__APP_VERSION__ =
  "0.0.0-test";
