// TODO(phase3): NEED_SDK_CROSS_APP_COMPONENTS — finder components
import type { ReactNode } from "react";

export function FileDetailsModal(): null {
  return null;
}

export function useFileDetailsModal(): { open: () => void; close: () => void } {
  return { open: () => {}, close: () => {} };
}

/** Stub: returns the raw file path until finder cross-app component is available. */
export function getMediaFileLocator(file: { path: string }): string {
  return file.path;
}

/** Stub: renders nothing until finder cross-app component is available. */
export function FileDetailsTooltipContent(_props: {
  file: unknown;
}): ReactNode {
  return null;
}
