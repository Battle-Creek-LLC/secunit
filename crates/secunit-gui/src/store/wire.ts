// Wires Tauri webview events to the store. Called from App on mount
// after the project is loaded. Returns a cleanup that detaches all
// listeners — call it on unmount or before priming a new project.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { store } from "./index";
import { TOPICS, type WatcherEvent } from "./events";

export async function wireWatcherEvents(): Promise<() => void> {
  const unlisteners: UnlistenFn[] = [];
  for (const topic of TOPICS) {
    const un = await listen<WatcherEvent>(topic, async (msg) => {
      try {
        await store.apply(msg.payload);
      } catch (err) {
        // Surfacing through console keeps the watcher loop alive — a
        // failure to refetch one slice should not freeze the UI.
        // eslint-disable-next-line no-console
        console.error("apply watcher event failed:", topic, err);
      }
    });
    unlisteners.push(un);
  }
  return () => {
    for (const un of unlisteners) un();
  };
}
