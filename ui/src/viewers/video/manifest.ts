import { Film } from "lucide-react";
import type { AppManifest } from "../../_framework/types";

export const manifest: AppManifest = {
  id: "viewer-video",
  category: "app",
  windowType: "tokimo-video-viewer",
  component: () => import("./VideoViewer"),
  defaultSize: { width: 960, height: 600 },
  icon: Film,
  color: "#ef4444",
};
