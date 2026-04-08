import { Spin } from "@tokiomo/components";
import { Film, Plus } from "lucide-react";
import { Suspense, useEffect, useRef, useState } from "react";
import { api } from "@/generated/rust-api";
import { useContainerWidth } from "@/shared/hooks/use-container-width";
import { useWindowNav } from "@/system";
import VideoContent from "./VideoContent";
import VideoSettingsModal from "./VideoSettingsModal";
import VideoSidebar from "./VideoSidebar";

const STORAGE_KEY = "video-active-category";

const LoadingFallback = (
  <div className="flex h-full items-center justify-center">
    <Spin />
  </div>
);

export default function VideoApp() {
  const { LazyViewComponent, route, navigate, updateTitle } = useWindowNav();
  const { data: categories, isLoading } = api.video.list.useQuery();
  const [containerRef, containerWidth] = useContainerWidth();
  const sidebarCollapsed = containerWidth > 0 && containerWidth < 720;
  const [activeCategoryId, setActiveCategoryId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const initialized = useRef(false);

  useEffect(() => {
    if (!categories?.length || initialized.current) return;
    initialized.current = true;
    const saved = localStorage.getItem(STORAGE_KEY);
    const id =
      saved && categories.some((c) => c.id === saved)
        ? saved
        : categories[0].id;
    setActiveCategoryId(id);
  }, [categories]);

  const activeCategory = categories?.find((c) => c.id === activeCategoryId);

  // Keep window title in sync with the active library when on the root route
  useEffect(() => {
    if (route === "/" && activeCategory) {
      updateTitle(`TokimoVideo · ${activeCategory.name}`);
    }
  }, [route, activeCategory, updateTitle]);

  const handleSelectCategory = (id: string) => {
    setActiveCategoryId(id);
    localStorage.setItem(STORAGE_KEY, id);
    if (route !== "/") {
      navigate("/");
    }
  };

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!categories?.length) {
    return (
      <>
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
            onClick={() => setSettingsOpen(true)}
            className="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-purple-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-purple-700"
          >
            <Plus className="h-4 w-4" />
            新建视频库
          </button>
        </div>
        <VideoSettingsModal
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
        />
      </>
    );
  }

  const isDetailPage = route !== "/" && LazyViewComponent;

  return (
    <>
      <div
        ref={containerRef}
        className="grid h-full"
        style={{ gridTemplateColumns: `${sidebarCollapsed ? 48 : 200}px 1fr` }}
      >
        <VideoSidebar
          categories={categories}
          activeId={activeCategoryId}
          onSelect={handleSelectCategory}
          collapsed={sidebarCollapsed}
          onCreateClick={() => setSettingsOpen(true)}
          onSettingsClick={() => setSettingsOpen(true)}
        />
        <div
          className={`min-w-0 flex-1 overflow-auto${isDetailPage ? " px-3 py-3 lg:px-4 lg:py-4" : ""}`}
        >
          {isDetailPage ? (
            <Suspense fallback={LoadingFallback}>
              <LazyViewComponent />
            </Suspense>
          ) : (
            activeCategoryId &&
            activeCategory && <VideoContent category={activeCategory} />
          )}
        </div>
      </div>
      <VideoSettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </>
  );
}
