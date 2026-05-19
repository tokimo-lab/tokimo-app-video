import { useCallback, useEffect, useState } from "react";

/**
 * Sidebar collapsed state — mirrors the host shell's `useSidebarCollapsed`
 * signature: `useSidebarCollapsed(componentId, autoCollapsed)` →
 * `{ collapsed, onToggleCollapse }`.
 *
 * Storage: localStorage keyed by componentId. The host shell uses a DB-backed
 * preference; the SDK doesn't expose preferences yet, so this is a temporary
 * localStorage fallback.
 */
export function useSidebarCollapsed(
  componentId: string,
  autoCollapsed: boolean,
): { collapsed: boolean; onToggleCollapse: () => void } {
  const storageKey = `video-app:sidebar-collapsed:${componentId}`;

  const [manuallyCollapsed, setManuallyCollapsed] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    try {
      return window.localStorage.getItem(storageKey) === "1";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      window.localStorage.setItem(storageKey, manuallyCollapsed ? "1" : "0");
    } catch {
      // ignore quota / privacy errors
    }
  }, [manuallyCollapsed, storageKey]);

  const collapsed = autoCollapsed || manuallyCollapsed;

  const onToggleCollapse = useCallback(() => {
    setManuallyCollapsed(!collapsed);
  }, [collapsed]);

  return { collapsed, onToggleCollapse };
}
