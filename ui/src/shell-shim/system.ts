/**
 * Compatibility proxy for legacy `shell-shim/system` import path.
 *
 * Most hooks now resolve to real implementations from `@tokimo/sdk`. A few
 * (player, background art, video-ui state, broadcast app events) are still
 * stubbed pending later batches; their stubs return safe defaults and live
 * locally so existing call sites continue to compile without surprises.
 */

import type { WindowState as SdkWindowState } from "@tokimo/sdk";
import {
  SYSTEM_CLOSE_EVENT as SDK_SYSTEM_CLOSE_EVENT,
  type MenuBarConfig as SdkMenuBarConfig,
  PickCancelled as SdkPickCancelled,
  emitBridge as sdkEmitBridge,
  emitPick as sdkEmitPick,
  pickWithBridge as sdkPickWithBridge,
  useBridge as sdkUseBridge,
  useBridgeSubscribe as sdkUseBridgeSubscribe,
  useMenuBar as sdkUseMenuBar,
  useToast as sdkUseToast,
  useWindowActions as sdkUseWindowActions,
  useWindowId as sdkUseWindowId,
  useWindowNav as sdkUseWindowNav,
} from "@tokimo/sdk";

// ── Re-exports from the SDK (real implementations) ─────────────────────────

export type WindowState = SdkWindowState;
export type MenuBarConfig = SdkMenuBarConfig;

export const SYSTEM_CLOSE_EVENT = SDK_SYSTEM_CLOSE_EVENT;
export const PickCancelled = SdkPickCancelled;

export const useWindowNav = sdkUseWindowNav;
export const useWindowActions = sdkUseWindowActions;
export const useWindowId = sdkUseWindowId;
export const useMessage = sdkUseToast;
export const useMenuBar = sdkUseMenuBar;
export const useBridge = sdkUseBridge;
export const useBridgeSubscribe = sdkUseBridgeSubscribe;
export const emitBridge = sdkEmitBridge;
export const emitPick = sdkEmitPick;
export const pickWithBridge = sdkPickWithBridge;

// ── Local stubs (deferred to later batches) ────────────────────────────────
//
// These return safe defaults so the standalone video bundle keeps running
// while the corresponding host integrations are designed.

interface UseAuthResult {
  user: { id: string; username?: string } | null;
  isLoading: boolean;
}

/** TODO(phase4b): wire to host useAuth via shell. */
export function useAuth(): UseAuthResult {
  return { user: null, isLoading: false };
}

/** TODO(phase4b): wire to i18next via shell.locale. */
export function useLang(): string {
  return "en";
}

/** TODO(phase4b): host theme snapshot. */
export function useTheme(): { theme: "light" | "dark" } {
  return { theme: "light" };
}

interface UseDateFormatResult {
  formatDate: (d: Date | string | number) => string;
}

/** TODO(phase4b): hook into host useDateFormat. */
export function useDateFormat(): UseDateFormatResult {
  return {
    formatDate: (d) => {
      try {
        return new Date(d).toLocaleString();
      } catch {
        return String(d);
      }
    },
  };
}

interface BackgroundArtApi {
  setArt: (url: string | null) => void;
}

/** TODO(phase4b): wire to host BackgroundArtProvider. */
export function useBackgroundArt(): BackgroundArtApi {
  return { setArt: () => {} };
}

/** TODO(phase4c): wire to host PlayerProvider. */
export function usePlayer(): {
  play: (...args: unknown[]) => void;
  pause: () => void;
  isPlaying: boolean;
  currentTime: number;
} {
  return {
    play: () => {},
    pause: () => {},
    isPlaying: false,
    currentTime: 0,
  };
}

/** TODO(phase4c): video-local UI state context. */
export function useVideoUiState(): {
  episodeListOpen: boolean;
  setEpisodeListOpen: (open: boolean) => void;
} {
  return {
    episodeListOpen: false,
    setEpisodeListOpen: () => {},
  };
}

interface WsJobEventLike {
  type: string;
  [key: string]: unknown;
}

/** TODO(phase4b): wire to shell.jobEvents. */
export function useAppEvent(_onEvent: (event: WsJobEventLike) => void): void {
  // no-op stub
}

// Legacy alias used by a couple of files; mirrors host's resolveWindowAppName
// which translates the i18n key in `WindowState.appName`. Without i18n
// integration here, fall back to the raw key.
export function resolveWindowAppName(win: WindowState): string {
  return win.appName ?? "Unknown";
}
