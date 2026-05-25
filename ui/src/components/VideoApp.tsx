import { useRuntimeCtx, useWindowActions, useWindowId } from "@tokimo/sdk";
import { AppSetupGuide, Spin } from "@tokimo/ui";
import { Film, Import, ListVideo, Plus } from "lucide-react";
import { Suspense, useCallback, useEffect } from "react";
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

  const syncProgress = useVideoLibraryProgress(categories);

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
