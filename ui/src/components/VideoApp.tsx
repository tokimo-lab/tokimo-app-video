import { useQueryClient } from "@tanstack/react-query";
import { Spin } from "@tokimo/ui";
import { Film, Plus } from "lucide-react";
import { Suspense, useCallback, useEffect } from "react";
import { api } from "@/generated/rust-api";
import { useContainerWidth } from "@/shared/hooks/use-container-width";
import { useSidebarCollapsed } from "@/shared/hooks/use-sidebar-collapsed";
import { useSyncProgress } from "@/shared/hooks/use-sync-progress";
import { useWindowActions, useWindowId, useWindowNav } from "@/system";
import type { TaskMetadata } from "@/system/window/window-types";
import { useSetActiveLibrary } from "./ActiveLibraryContext";
import VideoContent from "./VideoContent";
import VideoSidebar from "./VideoSidebar";

/** See PHOTO_SCAN_JOB_TYPES for rationale. Backend: apps/video/handlers/sync.rs */
const VIDEO_SCAN_JOB_TYPES = ["movie_scrape", "tv_scrape"] as const;

const LoadingFallback = (
  <div className="flex h-full items-center justify-center">
    <Spin />
  </div>
);

export default function VideoApp() {
  const { LazyViewComponent, params, replace, updateTitle } = useWindowNav();
  const { data: categories, isLoading } = api.video.list.useQuery();
  const [containerRef, containerWidth] = useContainerWidth();
  const { collapsed: sidebarCollapsed, onToggleCollapse } = useSidebarCollapsed(
    "video",
    containerWidth > 0 && containerWidth < 720,
  );

  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();

  // Active category is stored in the window route (persisted in DB via user_tasks).
  const activeCategoryId = params.categoryId ?? null;

  // Auto-select: navigate to the first valid category when none is in the route.
  useEffect(() => {
    if (!categories?.length) return;
    if (params.categoryId) {
      // Validate the stored category still exists; redirect to first if not.
      const valid = categories.some((c) => c.id === params.categoryId);
      if (!valid) replace(`/library/${categories[0].id}`);
      return;
    }
    replace(`/library/${categories[0].id}`);
  }, [categories, params.categoryId, replace]);

  const openLibraryEditor = useCallback(
    (videoId?: string) => {
      openModalWindow({
        component: () => import("@/apps/settings/admin/VideoLibraryWindow"),
        parentWindowId: windowId,
        title: videoId ? "编辑视频库" : "新建视频库",
        width: 680,
        height: 620,
        noResize: true,
        noMinimize: true,
        metadata: videoId
          ? ({ videoId } as Record<string, unknown> as TaskMetadata)
          : undefined,
      });
    },
    [openModalWindow, windowId],
  );

  const handleCreate = useCallback(
    () => openLibraryEditor(),
    [openLibraryEditor],
  );

  const handleSettings = useCallback(() => {
    openModalWindow({
      component: () => import("@/apps/settings/admin/VideoSettingsPage"),
      parentWindowId: windowId,
      title: "TokimoVideo 设置",
      width: 960,
      height: 640,
      noMinimize: true,
    });
  }, [openModalWindow, windowId]);

  const activeCategory = categories?.find((c) => c.id === activeCategoryId);

  // Sync active library to module-level store (consumed by VideoMenuBar)
  useSetActiveLibrary(activeCategory?.id, activeCategory?.type);

  // Keep window title in sync with active library
  useEffect(() => {
    if (activeCategory) {
      updateTitle(`TokimoVideo · ${activeCategory.name}`);
    }
  }, [activeCategory, updateTitle]);

  const handleSelectCategory = (id: string) => {
    replace(`/library/${id}`);
  };

  // ── Sync progress tracking (WS-driven + fallback polling) ──
  const queryClient = useQueryClient();

  const syncProgress = useSyncProgress({
    libraries: categories,
    progressQueryKey: (id) => api.video.getSyncProgress.queryKey({ id }),
    fetchProgress: (id) => api.video.getSyncProgress.fetch({ id }),
    scanJobTypes: VIDEO_SCAN_JOB_TYPES,
    onContentRefresh: () => {
      api.video.listVideoItems.invalidate(queryClient);
      api.video.listTvShows.invalidate(queryClient);
      api.video.getRecentlyAdded.invalidate(queryClient);
      api.video.listGenres.invalidate(queryClient);
      api.video.listCountries.invalidate(queryClient);
    },
    onLibraryRefresh: () => {
      api.video.list.invalidate(queryClient);
    },
  });

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!categories?.length) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 px-8 text-center">
        <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-purple-100 text-purple-600 dark:bg-purple-900/30 dark:text-purple-400">
          <Film className="h-8 w-8" />
        </div>
        <div>
          <h2 className="text-lg font-semibold text-fg-primary">
            开始使用 TokimoVideo
          </h2>
          <p className="mt-1 text-sm text-fg-muted">
            创建一个视频库来管理你的影视资源
          </p>
        </div>
        <button
          type="button"
          onClick={handleCreate}
          className="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-purple-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-purple-700"
        >
          <Plus className="h-4 w-4" />
          新建视频库
        </button>
      </div>
    );
  }

  // Detail routes (/movies/:videoItemId, /tv/:tvShowId) are rendered directly
  // by PageViewRouter; VideoApp is only the view for "/" and "/library/:categoryId".
  const isDetailPage = !!(params.videoItemId ?? params.tvShowId);

  return (
    <div ref={containerRef} className="relative flex h-full">
      <VideoSidebar
        categories={categories}
        activeId={activeCategoryId}
        onSelect={handleSelectCategory}
        collapsed={sidebarCollapsed}
        onCreateClick={handleCreate}
        onSettingsClick={handleSettings}
        syncProgress={syncProgress}
        onToggleCollapse={onToggleCollapse}
      />
      <div
        className={`min-w-0 flex-1 overflow-auto${isDetailPage ? " px-3 py-3 lg:px-4 lg:py-4" : ""}`}
      >
        {isDetailPage && LazyViewComponent ? (
          <Suspense fallback={LoadingFallback}>
            <LazyViewComponent />
          </Suspense>
        ) : (
          activeCategoryId &&
          activeCategory && (
            <VideoContent
              key={activeCategoryId}
              category={activeCategory}
              syncing={!!syncProgress[activeCategoryId]?.isActive}
            />
          )
        )}
      </div>
    </div>
  );
}
