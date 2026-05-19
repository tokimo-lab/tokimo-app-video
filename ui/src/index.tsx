import { type Dispose, defineApp } from "@tokimo/sdk";
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import "./index.css";

function VideoPlaceholder({ version }: { version: string }) {
  return (
    <div className="flex h-full w-full items-center justify-center bg-background text-foreground">
      <div className="max-w-md px-6 py-8 text-center">
        <div className="mb-3 text-2xl font-semibold">
          Video app — sidecar UI loaded
        </div>
        <div className="text-sm text-muted-foreground">
          Phase 2B work-in-progress. Frontend migration pending Phase 3.
        </div>
        <div className="mt-4 text-xs text-muted-foreground/70">
          bundle v{version}
        </div>
      </div>
    </div>
  );
}

export default defineApp({
  id: "video",
  manifest: {
    id: "video",
    appName: "Video",
    icon: "Film",
    color: "#a855f7",
    windowType: "video",
    defaultSize: { width: 1280, height: 800 },
    category: "app",
  },
  mount(container, _ctx): Dispose {
    const root: Root = createRoot(container);
    root.render(
      <StrictMode>
        <VideoPlaceholder version="0.1.0" />
      </StrictMode>,
    );
    return () => root.unmount();
  },
});
