import { useWindowActions, useWindowId, useWindowNav } from "@tokimo/sdk";
import { lazy, useCallback, useMemo } from "react";
import { matchRoute } from "./route-matcher";
import { VIDEO_VIEWS } from "./views";

type LazyComponentFactory = () => Promise<{ default: React.ComponentType }>;
type CachedLazyComponent = React.LazyExoticComponent<React.ComponentType>;

/**
 * Cache lazy components by factory reference so that two route patterns
 * pointing to the same factory return the identical React.lazy wrapper.
 * This prevents React from treating them as different component types
 * (which would trigger unmount/remount across routes backed by the same component).
 */
const factoryToLazy = new Map<LazyComponentFactory, CachedLazyComponent>();
function getLazy(factory: LazyComponentFactory): CachedLazyComponent {
  let comp = factoryToLazy.get(factory);
  if (!comp) {
    comp = lazy(factory);
    factoryToLazy.set(factory, comp);
  }
  return comp;
}

export interface VideoNavResult {
  /** Current route path */
  route: string;
  /** Params extracted from route matching (e.g. { videoItemId: "abc" }) */
  params: Record<string, string>;
  /** Navigate within this window (push to nav stack) */
  navigate: (route: string, title?: string) => void;
  /** Replace current route without pushing to nav stack */
  replace: (route: string, title?: string) => void;
  /** Go back in this window */
  goBack: () => void;
  /** Whether there's a previous view to go back to */
  canGoBack: boolean;
  /** Update the window title without changing the route */
  updateTitle: (title: string) => void;
  /** Open a new window */
  openWindow: ReturnType<typeof useWindowActions>["openWindow"];
  /** Open a modal window */
  openModalWindow: ReturnType<typeof useWindowActions>["openModalWindow"];
  /** Resolved view component factory for the current route (null if no match) */
  ViewComponent: LazyComponentFactory | null;
  /** Stable React.lazy wrapper for the current route's view component */
  LazyViewComponent: CachedLazyComponent | null;
}

export function useVideoNav(): VideoNavResult {
  const { route, navigate, replace, goBack, canGoBack } = useWindowNav();
  const { openWindow, openModalWindow } = useWindowActions();
  const windowId = useWindowId();

  const { params, ViewComponent, LazyViewComponent } = useMemo(() => {
    const patterns = Object.keys(VIDEO_VIEWS);
    const match = matchRoute(route, patterns);
    if (!match) {
      return { params: {}, ViewComponent: null, LazyViewComponent: null };
    }
    const factory = VIDEO_VIEWS[match.pattern] ?? null;
    return {
      params: match.params,
      ViewComponent: factory,
      LazyViewComponent: factory ? getLazy(factory) : null,
    };
  }, [route]);

  // updateTitle: replace the current route with a new title (no nav stack change)
  const updateTitle = useCallback(
    (title: string) => {
      replace(route, title);
    },
    [replace, route],
  );

  // Suppress unused variable warning — windowId is consumed indirectly via
  // useWindowId() to ensure correct context; openModalWindow already captures it.
  void windowId;

  return useMemo(
    () => ({
      route,
      params,
      navigate,
      replace,
      goBack,
      canGoBack,
      updateTitle,
      openWindow,
      openModalWindow,
      ViewComponent,
      LazyViewComponent,
    }),
    [
      route,
      params,
      navigate,
      replace,
      goBack,
      canGoBack,
      updateTitle,
      openWindow,
      openModalWindow,
      ViewComponent,
      LazyViewComponent,
    ],
  );
}
