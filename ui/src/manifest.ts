import { Clapperboard } from "lucide-react";
import type { AppManifest } from "../_framework/types";

export const manifest: AppManifest = {
  id: "video",
  category: "system",
  fullBleed: true,
  defaultSize: { width: 1200, height: 800 },
  icon: Clapperboard,
  image: "/page-icons/video.png",
  color: "#e11d48",
  appName: "dashboard.menu.video",
  order: 1,
  component: () => import("./components/VideoApp"),
  menuBar: () => import("./components/VideoMenuBar"),
  views: {
    "/": () => import("./components/VideoApp"),
    "/movies/:videoItemId": () => import("../media/pages/VideoItemDetailPage"),
    "/tv/:tvShowId": () => import("../media/pages/TvShowDetailPage"),
  },

  userSettings: {
    order: 10,
    libraryDomain: "video",
    sections: [
      {
        key: "display",
        label: "settings.library.display",
        fields: [
          {
            key: "defaultSort",
            type: "select",
            label: "settings.library.defaultSort",
            defaultValue: "addedAt",
            options: [
              { label: "settings.library.sortAddedAt", value: "addedAt" },
              { label: "settings.library.sortTitleAsc", value: "title_asc" },
              {
                label: "settings.library.sortTitleDesc",
                value: "title_desc",
              },
              { label: "settings.library.sortYearDesc", value: "year_desc" },
              { label: "settings.library.sortYearAsc", value: "year_asc" },
              { label: "settings.library.sortRating", value: "rating" },
            ],
          },
        ],
      },
    ],
  },
};
