/**
 * Adult-mode flag.
 *
 * Host shell version: backed by `api.auth.generalSettings` (env flag +
 * DB toggle). The video app currently has no direct access to that
 * endpoint from the SDK, so this stub reads a localStorage cache written
 * by the host shell on app startup.
 *
 * TODO(post-extraction): replace with a typed SDK call once the SDK
 * exposes `ctx.shell.appearance.adultMode`.
 */
export function useAdultMode(): {
  enabled: boolean;
  visible: boolean;
  isLoading: boolean;
} {
  if (typeof window === "undefined") {
    return { enabled: false, visible: false, isLoading: false };
  }
  try {
    const enabled =
      window.localStorage.getItem("appearance:adult-mode") === "1";
    const visible =
      window.localStorage.getItem("appearance:adult-mode-visible") === "1";
    return { enabled, visible, isLoading: false };
  } catch {
    return { enabled: false, visible: false, isLoading: false };
  }
}
