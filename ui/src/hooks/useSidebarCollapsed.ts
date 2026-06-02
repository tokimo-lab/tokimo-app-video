import { useRuntimeCtx, useShellPreference } from "@tokimo/sdk";
import { useCallback } from "react";

/**
 * Sidebar collapsed state — backed by the host shell's DB preference system
 * via `ctx.shell.preferences` (scope = "app", scopeId = appId).
 *
 * Combines auto-collapse (e.g. < 720px) with manual user override:
 * - Auto-collapse only when the container is narrow AND no manual lock.
 * - If the user manually collapses, it stays collapsed regardless of width.
 * - Clicking the expand button releases the manual lock.
 */
export function useSidebarCollapsed(
  componentId: string,
  autoCollapsed: boolean,
): { collapsed: boolean; onToggleCollapse: () => void } {
  const ctx = useRuntimeCtx();
  const { data, patch } = useShellPreference<{
    sidebar?: Record<string, { sidebarCollapsed?: boolean }>;
  }>(ctx);

  const manuallyCollapsed =
    data.sidebar?.[componentId]?.sidebarCollapsed === true;
  const collapsed = autoCollapsed || manuallyCollapsed;

  const onToggleCollapse = useCallback(() => {
    patch({
      sidebar: { [componentId]: { sidebarCollapsed: !collapsed } },
    });
  }, [collapsed, componentId, patch]);

  return { collapsed, onToggleCollapse };
}
