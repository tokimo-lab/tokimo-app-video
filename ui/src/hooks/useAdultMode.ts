import { useRuntimeCtx, useShellGeneralSettings } from "@tokimo/sdk";

/**
 * Adult-mode flag — backed by the host shell's general settings API
 * via `ctx.shell.generalSettings`.
 *
 * Requires env ADULT_MODE_ENABLED=true + DB GeneralSettings.adultModeEnabled.
 */
export function useAdultMode(): {
  enabled: boolean;
  visible: boolean;
  isLoading: boolean;
} {
  const ctx = useRuntimeCtx();
  const settings = useShellGeneralSettings(ctx);
  return {
    enabled: settings.adultModeEnabled,
    visible: settings.adultModeVisible,
    isLoading: false,
  };
}
