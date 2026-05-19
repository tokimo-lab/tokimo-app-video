/**
 * Local stubs for host system hooks that haven't been wired through @tokimo/sdk yet.
 * Each stub returns a safe default so the standalone video bundle compiles and
 * renders without crashing. Replace with real shell integrations as host APIs
 * are exposed.
 */

import type { WindowState } from "@tokimo/sdk";
import type { RefObject } from "react";

interface UseAuthResult {
  user: { id: string; username?: string } | null;
  isLoading: boolean;
}

/** TODO(phase4b): wire to host useAuth via shell. */
export function useAuth(): UseAuthResult {
  return { user: null, isLoading: false };
}

/** TODO(phase4b): wire to i18next via shell.locale. */
export function useLang(): { lang: string } {
  return { lang: "en" };
}

/** TODO(phase4b): host theme snapshot. */
export function useTheme(): { theme: "light" | "dark" } {
  return { theme: "light" };
}

interface UseDateFormatResult {
  formatLong: (d: Date | string | number) => string;
}

/** TODO(phase4b): hook into host useDateFormat. */
export function useDateFormat(): UseDateFormatResult {
  return {
    formatLong: (d) => {
      try {
        return new Date(d).toLocaleString();
      } catch {
        return String(d);
      }
    },
  };
}

interface BackgroundArtApi {
  setBackgroundArt: (url: string | null) => void;
}

/** TODO(phase4b): wire to host BackgroundArtProvider. */
export function useBackgroundArt(): BackgroundArtApi {
  return { setBackgroundArt: () => {} };
}

interface PlayerApi {
  play: (...args: unknown[]) => void;
  pause: () => void;
  isPlaying: boolean;
  currentTime: number;
  item: {
    tvShowId?: string;
    episodeId?: string;
    [key: string]: unknown;
  } | null;
}

/** TODO(phase4c): wire to host PlayerProvider. */
export function usePlayer(): PlayerApi {
  return {
    play: () => {},
    pause: () => {},
    isPlaying: false,
    currentTime: 0,
    item: null,
  };
}

interface VideoUiState {
  episodeListOpen: boolean;
  setEpisodeListOpen: (open: boolean) => void;
  onEndedRef: RefObject<(() => void) | null>;
}

const DEFAULT_ON_ENDED_REF: RefObject<(() => void) | null> = { current: null };

/** TODO(phase4c): video-local UI state context. */
export function useVideoUiState(): VideoUiState {
  return {
    episodeListOpen: false,
    setEpisodeListOpen: () => {},
    onEndedRef: DEFAULT_ON_ENDED_REF,
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

/**
 * Legacy alias used by a couple of files; mirrors host's resolveWindowAppName
 * which translates the i18n key in WindowState.appName. Without i18n
 * integration here, fall back to the raw key.
 */
export function resolveWindowAppName(win: WindowState): string {
  return win.appName ?? "Unknown";
}
