import { useRuntimeCtx, useWindowActions, useWindowId } from "@tokimo/sdk";
import { AppSetupGuide, cn, Spin } from "@tokimo/ui";
import { Film, Import, ListVideo, Plus } from "lucide-react";
import { Suspense, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useContainerWidth } from "../hooks/useContainerWidth";
import { useSidebarCollapsed } from "../hooks/useSidebarCollapsed";
import { useVideoLibraryProgress } from "../hooks/useVideoLibraryProgress";
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
  const mobile = containerWidth > 0 && containerWidth < 720;
  const { collapsed: sidebarCollapsed, onToggleCollapse } = useSidebarCollapsed(
    "video",
    mobile,
  );

  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();
  const ctx = useRuntimeCtx();

  // Active category is stored in the window route (persisted in DB via user_tasks).
  const [sourceCategoryId, setSourceCategoryId] = useState<string | null>(
    params.categoryId ?? null,
  );
  const routeCategoryId = params.categoryId ?? null;
  if (routeCategoryId && routeCategoryId !== sourceCategoryId) {
    setSourceCategoryId(routeCategoryId);
  }
  const activeCategoryId = routeCategoryId ?? sourceCategoryId;

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

  const syncProgress = useVideoLibraryProgress(categories);

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center bg-[var(--color-surface-content)] pt-[var(--app-safe-area-top,0px)] pr-[var(--app-safe-area-right,0px)] pb-[var(--app-safe-area-bottom,0px)] pl-[var(--app-safe-area-left,0px)]">
        <Spin />
      </div>
    );
  }

  if (!categories?.length) {
    return (
      <AppSetupGuide
        className="pt-[var(--app-safe-area-top,0px)] pr-[var(--app-safe-area-right,0px)] pb-[var(--app-safe-area-bottom,0px)] pl-[var(--app-safe-area-left,0px)]"
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

  const sidebar = (
    <VideoSidebar
      categories={categories}
      activeId={activeCategoryId}
      onSelect={handleSelectCategory}
      collapsed={sidebarCollapsed}
      mobile={mobile}
      onCreateClick={() => openEditorModal()}
      onSettingsClick={() =>
        activeCategoryId && openEditorModal({ videoId: activeCategoryId })
      }
      syncProgress={syncProgress}
      onToggleCollapse={onToggleCollapse}
    />
  );
  const mobileHeader = mobile ? (
    <div className="shrink-0 bg-surface-base pt-[var(--app-safe-area-top,0px)] pr-[var(--app-safe-area-right,0px)] pl-[var(--app-safe-area-left,0px)] [&>div:first-child]:bg-transparent">
      {sidebar}
    </div>
  ) : undefined;

  return (
    <div
      ref={containerRef}
      className={cn("relative flex h-full min-h-0", mobile && "flex-col")}
    >
      {!mobile && (
        <div className="flex shrink-0 pt-[var(--app-safe-area-top,0px)] pb-[var(--app-safe-area-bottom,0px)] pl-[var(--app-safe-area-left,0px)]">
          {sidebar}
        </div>
      )}
      <div
        className={cn(
          "relative min-h-0 min-w-0 flex-1 overflow-hidden bg-[var(--color-surface-content)]",
          !mobile && "[--app-safe-area-left:0px]",
        )}
      >
        {activeCategoryId && activeCategory && (
          <div
            className={`absolute inset-0${isDetailPage ? " invisible" : ""}`}
            inert={isDetailPage}
            aria-hidden={isDetailPage}
          >
            <VideoContent
              key={activeCategoryId}
              category={activeCategory}
              active={!isDetailPage}
              header={mobileHeader}
              syncing={!!syncProgress[activeCategoryId]?.isActive}
            />
          </div>
        )}
        {isDetailPage && LazyViewComponent && (
          <div className="absolute inset-0 overflow-y-auto">
            {mobileHeader}
            <div
              className={cn(
                "pr-[calc(0.75rem+var(--app-safe-area-right,0px))] pb-[calc(0.75rem+var(--app-safe-area-bottom,0px))] pl-[calc(0.75rem+var(--app-safe-area-left,0px))] lg:pr-[calc(1rem+var(--app-safe-area-right,0px))] lg:pb-[calc(1rem+var(--app-safe-area-bottom,0px))] lg:pl-[calc(1rem+var(--app-safe-area-left,0px))]",
                mobile
                  ? "pt-3"
                  : "pt-[calc(0.75rem+var(--app-safe-area-top,0px))] lg:pt-[calc(1rem+var(--app-safe-area-top,0px))]",
              )}
            >
              <Suspense fallback={LoadingFallback}>
                <LazyViewComponent />
              </Suspense>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
