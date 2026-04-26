import { useQueryClient } from "@tanstack/react-query";
import { Spin } from "@tokimo/ui";
import { Film, Plus } from "lucide-react";
import { Suspense, useCallback, useEffect, useState } from "react";
import { AnimatedSettingsPane } from "@/apps/_framework/AnimatedSettingsPane";
import VideoLibraryEditor from "@/apps/settings/admin/VideoLibraryEditor";
import { api } from "@/generated/rust-api";
import { useContainerWidth } from "@/shared/hooks/use-container-width";
import { useSidebarCollapsed } from "@/shared/hooks/use-sidebar-collapsed";
import { useSyncProgress } from "@/shared/hooks/use-sync-progress";
import { useWindowNav } from "@/system";
import { useSetActiveLibrary } from "./ActiveLibraryContext";
import VideoContent from "./VideoContent";
import VideoSidebar from "./VideoSidebar";

/** See PHOTO_SCAN_JOB_TYPES for rationale. Backend: apps/video/handlers/sync.rs */
const VIDEO_SCAN_JOB_TYPES = ["movie_scrape", "tv_scrape"] as const;

type ViewMode = "content" | "settings" | "settings-new";

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
  const [mode, setMode] = useState<ViewMode>("content");

  // Active category is stored in the window route (persisted in DB via user_tasks).
  const activeCategoryId = params.categoryId ?? null;

  // Detail routes (/movies/:videoItemId, /tv/:tvShowId)
  const isDetailPage = !!(params.videoItemId ?? params.tvShowId);

  // Auto-select first category when none in route
  useEffect(() => {
    if (!categories?.length) return;
    if (params.categoryId) {
      const valid = categories.some((c) => c.id === params.categoryId);
      if (!valid) replace(`/library/${categories[0].id}`);
      return;
    }
    if (!isDetailPage) {
      replace(`/library/${categories[0].id}`);
    }
  }, [categories, params.categoryId, isDetailPage, replace]);

  const openSettings = useCallback(() => {
    if (isDetailPage && categories?.length) {
      replace(`/library/${categories[0].id}`);
    }
    setMode("settings");
  }, [isDetailPage, categories, replace]);

  const openCreate = useCallback(() => {
    if (isDetailPage && categories?.length) {
      replace(`/library/${categories[0].id}`);
    }
    setMode("settings-new");
  }, [isDetailPage, categories, replace]);

  const activeCategory = categories?.find((c) => c.id === activeCategoryId);

  // Sync active library to module-level store (consumed by VideoMenuBar)
  useSetActiveLibrary(activeCategory?.id, activeCategory?.type);

  useEffect(() => {
    if (isDetailPage) return;
    if (mode === "settings-new") {
      updateTitle("TokimoVideo · 新建视频库");
    } else if (mode === "settings" && activeCategory) {
      updateTitle(`TokimoVideo · ${activeCategory.name} · 设置`);
    } else if (activeCategory) {
      updateTitle(`TokimoVideo · ${activeCategory.name}`);
    }
  }, [activeCategory, mode, isDetailPage, updateTitle]);

  const handleSelectCategory = (id: string) => {
    replace(`/library/${id}`);
    setMode("content");
  };

  const handleSaved = (savedId: string) => {
    replace(`/library/${savedId}`);
    setMode("content");
  };

  const handleDeleted = () => {
    const remaining = (categories ?? []).filter(
      (c) => c.id !== activeCategoryId,
    );
    const next = remaining[0]?.id;
    if (next) {
      replace(`/library/${next}`);
    } else {
      replace("/");
    }
    setMode("content");
  };

  const handleCancel = () => {
    setMode("content");
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
    if (mode === "settings-new") {
      return (
        <div ref={containerRef} className="relative flex h-full">
          <VideoSidebar
            categories={[]}
            activeId={null}
            onSelect={handleSelectCategory}
            collapsed={sidebarCollapsed}
            onCreateClick={openCreate}
            onSettingsClick={openSettings}
            onToggleCollapse={onToggleCollapse}
            settingsActive
          />
          <div className="min-w-0 flex-1 overflow-hidden h-full">
            <VideoLibraryEditor
              key="__new__"
              onSaved={handleSaved}
              onCancel={handleCancel}
            />
          </div>
        </div>
      );
    }
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
          onClick={openCreate}
          className="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-purple-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-purple-700"
        >
          <Plus className="h-4 w-4" />
          新建视频库
        </button>
      </div>
    );
  }

  const isSettingsView = !isDetailPage && mode !== "content";

  return (
    <div ref={containerRef} className="relative flex h-full">
      <VideoSidebar
        categories={categories}
        activeId={activeCategoryId}
        onSelect={handleSelectCategory}
        collapsed={sidebarCollapsed}
        onCreateClick={openCreate}
        onSettingsClick={openSettings}
        syncProgress={syncProgress}
        onToggleCollapse={onToggleCollapse}
        settingsActive={isSettingsView}
      />
      <div
        className={`relative min-w-0 flex-1 overflow-auto${isDetailPage ? " px-3 py-3 lg:px-4 lg:py-4" : ""}`}
      >
        {isDetailPage && LazyViewComponent ? (
          <Suspense fallback={LoadingFallback}>
            <LazyViewComponent />
          </Suspense>
        ) : (
          <>
            {activeCategoryId && activeCategory && mode === "content" && (
              <VideoContent
                key={activeCategoryId}
                category={activeCategory}
                syncing={!!syncProgress[activeCategoryId]?.isActive}
              />
            )}
            <AnimatedSettingsPane open={mode === "settings-new"}>
              <VideoLibraryEditor
                key="__new__"
                onSaved={handleSaved}
                onCancel={handleCancel}
              />
            </AnimatedSettingsPane>
            <AnimatedSettingsPane
              open={mode === "settings" && !!activeCategoryId}
            >
              <VideoLibraryEditor
                key={activeCategoryId ?? "edit"}
                videoId={activeCategoryId ?? undefined}
                onSaved={handleSaved}
                onDeleted={handleDeleted}
                onCancel={handleCancel}
              />
            </AnimatedSettingsPane>
          </>
        )}
      </div>
    </div>
  );
}
