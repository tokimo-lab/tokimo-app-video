import { Empty, Spin } from "@tokiomo/components";
import { Suspense, useEffect, useRef, useState } from "react";
import { api } from "@/generated/rust-api";
import { useWindowNav } from "@/system";
import VideoContent from "./VideoContent";
import VideoSidebar from "./VideoSidebar";

const STORAGE_KEY = "video-active-category";

const LoadingFallback = (
  <div className="flex h-full items-center justify-center">
    <Spin />
  </div>
);

export default function VideoApp() {
  const { LazyViewComponent, route, navigate } = useWindowNav();
  const { data: categories, isLoading } = api.video.list.useQuery();
  const [activeCategoryId, setActiveCategoryId] = useState<string | null>(null);
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
      <div className="flex h-full items-center justify-center">
        <Empty description="还没有视频分类，请在系统设置中添加" />
      </div>
    );
  }

  const activeCategory = categories.find((c) => c.id === activeCategoryId);
  const isDetailPage = route !== "/" && LazyViewComponent;

  return (
    <div className="grid h-full" style={{ gridTemplateColumns: "200px 1fr" }}>
      <VideoSidebar
        categories={categories}
        activeId={activeCategoryId}
        onSelect={handleSelectCategory}
      />
      <div className="min-w-0 flex-1 overflow-auto">
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
  );
}
