// TODO(phase3): NEED_SDK_SYSTEM_HOOKS
// Until the standalone app gets its own system hooks (or SDK exports them),
// every system hook/constant call throws or returns safe defaults.

export function useAuth(): any {
  return { user: null, isLoading: false };
}

export function useMessage(): any {
  return {
    success: () => {},
    error: () => {},
    info: () => {},
    warning: () => {},
  };
}

export function useWindowManager(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useWindowManager called");
}

export function useTheme(): any {
  return { theme: "light" };
}

export function useWs(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useWs called");
}

export function useJobEvents(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useJobEvents called");
}

export function useMediaSession(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useMediaSession called");
}

export function useLang(): any {
  return "en";
}

export function usePlayer(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: usePlayer called");
}

export function useBackgroundArt(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useBackgroundArt called");
}

export function useWindowNav(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useWindowNav called");
}

export function useWindowActions(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useWindowActions called");
}

export function useWindowId(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useWindowId called");
}

export function useDateFormat(): any {
  return { formatDate: (d: any) => String(d) };
}

export function useVideoUiState(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useVideoUiState called");
}

export function useMenuBar(): any {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useMenuBar called");
}

export const SYSTEM_CLOSE_EVENT = "system:close";

export function emitBridge(): void {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: emitBridge called");
}

export function useBridge(): string {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useBridge called");
}

export function useBridgeSubscribe(): void {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: useBridgeSubscribe called");
}

export function pickWithBridge(): Promise<any> {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: pickWithBridge called");
}

export class PickCancelled extends Error {}

export function emitPick(): void {
  throw new Error("NEED_SDK_SYSTEM_HOOKS: emitPick called");
}

export function resolveWindowAppName(): string {
  return "Unknown";
}

export type MenuBarConfig = any;
