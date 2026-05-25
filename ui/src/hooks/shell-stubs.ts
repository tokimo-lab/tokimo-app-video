/**
 * Local stubs for host system hooks that haven't been wired through @tokimo/sdk yet.
 * Each stub returns a safe default so the standalone video bundle compiles and
 * renders without crashing. Replace with real shell integrations as host APIs
 * are exposed.
 */

import type {
  PlayerPlayMeta,
  PlayerSourceMetadata,
  ShellJobEvent,
  ShellPersonEvent,
  WindowState,
} from "@tokimo/sdk";
import { useShellApi } from "@tokimo/sdk";
import { useEffect, useRef, useSyncExternalStore } from "react";

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
  play: (
    file: unknown,
    meta: PlayerPlayMeta,
    options?: { initialPosition?: number; startPaused?: boolean },
  ) => Promise<void>;
  pause: () => void;
  isPlaying: boolean;
  currentTime: number;
  item: {
    fileId: string;
    sourceMetadata?: PlayerSourceMetadata;
  } | null;
}

/** TODO(phase4c): wire to host PlayerProvider. */
export function usePlayer(): PlayerApi {
  const shell = useShellApi();
  const item = useSyncExternalStore(
    shell.player.subscribeItem,
    shell.player.getCurrentItem,
    shell.player.getCurrentItem,
  );
  return {
    play: shell.player.play,
    pause: () => {},
    isPlaying: false,
    currentTime: 0,
    item,
  };
}

export function useAppEvent(onEvent: (event: ShellJobEvent) => void): void {
  const shell = useShellApi();
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  useEffect(() => {
    return shell.jobEvents.subscribe({
      onEvent: (event) => onEventRef.current(event),
    });
  }, [shell.jobEvents]);
}

export function usePersonEvents(
  onEvent: (event: ShellPersonEvent) => void,
): void {
  const shell = useShellApi();
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  useEffect(() => {
    return shell.personEvents.subscribe({
      onEvent: (event) => onEventRef.current(event),
    });
  }, [shell.personEvents]);
}

/**
 * Legacy alias used by a couple of files; mirrors host's resolveWindowAppName
 * which translates the i18n key in WindowState.appName. Without i18n
 * integration here, fall back to the raw key.
 */
export function resolveWindowAppName(win: WindowState): string {
  return win.appName ?? "Unknown";
}
