import { Film } from "lucide-react";
import type { AppManifest } from "../../_framework/types";

export const manifest: AppManifest = {
  id: "viewer-video",
  name: "视频播放器",
  category: "app",
  windowType: "video",
  component: () => import("./VideoViewer"),
  defaultSize: { width: 960, height: 600 },
  icon: Film,
  color: "#ef4444",
};
