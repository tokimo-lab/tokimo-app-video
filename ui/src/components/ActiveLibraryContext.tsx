/**
 * Module-level reactive store for the active video library.
 *
 * VideoMenuBar wraps VideoApp in the component tree (manifest menuBar),
 * so a React context from VideoApp can't reach VideoMenuBar.
 * We use useSyncExternalStore with a module-level snapshot instead.
 */
import { useCallback, useSyncExternalStore } from "react";

interface ActiveLibraryInfo {
  id: string | null;
  type: string | null;
}

const DEFAULT: ActiveLibraryInfo = { id: null, type: null };

let current: ActiveLibraryInfo = DEFAULT;
const listeners = new Set<() => void>();

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function getSnapshot(): ActiveLibraryInfo {
  return current;
}

export function setActiveLibrary(id: string | null, type: string | null) {
  if (current.id === id && current.type === type) return;
  current = { id, type };
  for (const cb of listeners) cb();
}

export function useActiveLibrary(): ActiveLibraryInfo {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Hook for VideoApp: keeps the store in sync with component state. */
export function useSetActiveLibrary(
  id: string | null | undefined,
  type: string | null | undefined,
) {
  const sync = useCallback(() => {
    setActiveLibrary(id ?? null, type ?? null);
  }, [id, type]);

  // Sync immediately on mount and when values change
  sync();
}
