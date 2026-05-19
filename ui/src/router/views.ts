/** Route → lazy component factory map for the video app. */

type LazyComponentFactory = () => Promise<{ default: React.ComponentType }>;

export const VIDEO_VIEWS: Record<string, LazyComponentFactory> = {
  "/": () => import("../components/VideoApp"),
  "/library/:categoryId": () => import("../components/VideoApp"),
  "/movies/:videoItemId": () => import("../detail-pages/VideoItemDetailPage"),
  "/tv/:tvShowId": () => import("../detail-pages/TvShowDetailPage"),
};
