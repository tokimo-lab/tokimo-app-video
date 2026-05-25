import { useQueryClient } from "@tanstack/react-query";
import {
  type ShellJobEvent,
  useJobProgress,
  useRuntimeCtx,
  useWindowActions,
  useWindowId,
} from "@tokimo/sdk";
import { AppSetupGuide, Spin } from "@tokimo/ui";
import { Film, Import, ListVideo, Plus } from "lucide-react";
import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useContainerWidth } from "../hooks/useContainerWidth";
import { useSidebarCollapsed } from "../hooks/useSidebarCollapsed";
import { registerBridge } from "../modal-bridge";
import { useVideoNav } from "../router/useVideoNav";
import { useSetActiveLibrary } from "./ActiveLibraryContext";
import VideoContent from "./VideoContent";
import VideoSidebar from "./VideoSidebar";

const LoadingFallback = (
  <div className="flex h-full items-center justify-center">
    <Spin />
  </div>
);

export default function VideoApp() {
  const { t } = useTranslation();
  const { LazyViewComponent, params, replace, updateTitle } = useVideoNav();
  const { data: categories, isLoading } = api.video.list.useQuery();
  const [containerRef, containerWidth] = useContainerWidth();
  const { collapsed: sidebarCollapsed, onToggleCollapse } = useSidebarCollapsed(
    "video",
    containerWidth > 0 && containerWidth < 720,
  );

  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();
  const ctx = useRuntimeCtx();

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

  const openEditorModal = useCallback(
    (opts: { videoId?: string } = {}) => {
      const bridgeId = registerBridge({
        kind: "library-editor",
        ctx,
        onSaved: () => {},
        onDeleted: () => {},
      });
      const metadata: Record<string, unknown> = { bridgeId };
      if (opts.videoId) metadata.videoId = opts.videoId;

      openModalWindow({
        component: () => import("./VideoLibraryEditorWindow"),
        parentWindowId: windowId,
        title: opts.videoId
          ? t("media.libraryEditor.settingsTitle")
          : t("media.libraryEditor.newTitle"),
        width: 720,
        height: 640,
        noResize: true,
        noMinimize: true,
        metadata,
      });
    },
    [ctx, openModalWindow, windowId, t],
  );

  const activeCategory = categories?.find((c) => c.id === activeCategoryId);

  // Sync active library to module-level store (consumed by VideoMenuBar)
  useSetActiveLibrary(activeCategory?.id, activeCategory?.type);

  useEffect(() => {
    if (isDetailPage) return;
    if (activeCategory) {
      updateTitle(`TokimoVideo · ${activeCategory.name}`);
    }
  }, [activeCategory, isDetailPage, updateTitle]);

  const handleSelectCategory = (id: string) => {
    replace(`/library/${id}`);
  };

  // ── Sync progress tracking (thin SDK helper + fallback polling) ──
  const queryClient = useQueryClient();
  const [activeLibIds, setActiveLibIds] = useState<Set<string>>(new Set());
  const [progressPct, setProgressPct] = useState<Record<string, number>>({});

  // Stable refs for content/library refresh callbacks
  const qcRef = useRef(queryClient);
  qcRef.current = queryClient;

  const refreshContent = useCallback(() => {
    const qc = qcRef.current;
    api.video.listVideoItems.invalidate(qc);
    api.video.listTvShows.invalidate(qc);
    api.video.getRecentlyAdded.invalidate(qc);
    api.video.listGenres.invalidate(qc);
    api.video.listCountries.invalidate(qc);
    api.video.list.invalidate(qc);
  }, []);

  // Throttle content refresh (max once per 500ms)
  const throttleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const throttlePendingRef = useRef(false);
  const throttledRefresh = useCallback(() => {
    if (throttleTimerRef.current) {
      throttlePendingRef.current = true;
      return;
    }
    refreshContent();
    throttleTimerRef.current = setTimeout(() => {
      throttleTimerRef.current = null;
      if (throttlePendingRef.current) {
        throttlePendingRef.current = false;
        refreshContent();
      }
    }, 500);
  }, [refreshContent]);

  // Shared event handler for both tv_scrape and movie_scrape
  const handleJobEvent = useCallback(
    (e: ShellJobEvent) => {
      const data = e.data as Record<string, unknown> | undefined;
      if (!data) return;
      const params = (data.params ?? data.payload ?? {}) as Record<
        string,
        unknown
      >;
      const libId = (e.appId ?? params.videoId ?? data.videoId) as
        | string
        | undefined;
      if (!libId) return;

      const status = data.status as string;
      if (
        status === "completed" ||
        status === "failed" ||
        status === "cancelled"
      ) {
        throttledRefresh();
        setActiveLibIds((prev) => {
          const next = new Set(prev);
          next.delete(libId);
          return next;
        });
      } else {
        setActiveLibIds((prev) => {
          if (prev.has(libId)) return prev;
          const next = new Set(prev);
          next.add(libId);
          return next;
        });
      }
    },
    [throttledRefresh],
  );

  useJobProgress("tv_scrape", handleJobEvent);
  useJobProgress("movie_scrape", handleJobEvent);

  // Polling fallback for progress percentages
  useEffect(() => {
    if (activeLibIds.size === 0) return;
    const interval = setInterval(async () => {
      const ids = Array.from(activeLibIds);
      const pcts: Record<string, number> = {};
      await Promise.all(
        ids.map(async (id) => {
          try {
            const data = await queryClient.fetchQuery({
              queryKey: api.video.getSyncProgress.queryKey({ id }),
              queryFn: () => api.video.getSyncProgress.fetch({ id }),
              staleTime: 1000,
            });
            const total =
              data.completed + data.running + data.pending + data.failed;
            pcts[id] =
              total > 0 ? Math.round((data.completed / total) * 100) : 0;
          } catch {
            pcts[id] = 0;
          }
        }),
      );
      setProgressPct(pcts);
    }, 5000);
    return () => clearInterval(interval);
  }, [activeLibIds, queryClient]);

  // Also seed activeIds from libraries already marked "syncing"
  useEffect(() => {
    if (!categories) return;
    const syncing = categories
      .filter((l) => l.syncStatus === "syncing")
      .map((l) => l.id);
    if (syncing.length > 0) {
      setActiveLibIds((prev) => {
        const next = new Set(prev);
        for (const id of syncing) next.add(id);
        return next.size !== prev.size ? next : prev;
      });
    }
  }, [categories]);

  // Build syncProgress map
  const syncProgress: Record<string, { isActive: boolean; pct: number }> = {};
  for (const id of activeLibIds) {
    syncProgress[id] = { isActive: true, pct: progressPct[id] ?? 0 };
  }

  // Cleanup throttle timer
  useEffect(() => {
    return () => {
      if (throttleTimerRef.current) clearTimeout(throttleTimerRef.current);
    };
  }, []);

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!categories?.length) {
    return (
      <AppSetupGuide
        imageSrc="/page-icons/video.png"
        accentColor="purple"
        title={t("common.setupGuide.getStarted", { name: "TokimoVideo" })}
        description={t("common.setupGuide.videoTagline")}
        features={(
          t("common.setupGuide.videoFeatures", {
            returnObjects: true,
          }) as string[]
        ).map((label, i) => ({
          icon: [Import, Film, ListVideo][i],
          label,
        }))}
        actionLabel={t("common.setupGuide.videoAction")}
        actionIcon={Plus}
        onAction={() => openEditorModal()}
      />
    );
  }

  return (
    <div ref={containerRef} className="relative flex h-full">
      <VideoSidebar
        categories={categories}
        activeId={activeCategoryId}
        onSelect={handleSelectCategory}
        collapsed={sidebarCollapsed}
        onCreateClick={() => openEditorModal()}
        onSettingsClick={() =>
          activeCategoryId && openEditorModal({ videoId: activeCategoryId })
        }
        syncProgress={syncProgress}
        onToggleCollapse={onToggleCollapse}
      />
      <div
        className={`relative min-w-0 flex-1 overflow-auto${isDetailPage ? " px-3 py-3 lg:px-4 lg:py-4" : ""}`}
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
