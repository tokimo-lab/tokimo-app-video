import { Clapperboard } from "lucide-react";
import type { AppManifest } from "../_framework/types";

export const manifest: AppManifest = {
  id: "video",
  name: "TokimoVideo",
  category: "system",
  fullBleed: true,
  defaultSize: { width: 1200, height: 800 },
  icon: Clapperboard,
  image: "/page-icons/video.png",
  color: "#e11d48",
  labelKey: "video",
  order: 1,
  component: () => import("./components/VideoApp"),
  menuBar: () => import("./components/VideoMenuBar"),
  views: {
    "/": () => import("./components/VideoApp"),
    "/movies/:videoItemId": () => import("../media/pages/MovieDetailPage"),
    "/tv/:tvShowId": () => import("../media/pages/TvShowDetailPage"),
  },
};
