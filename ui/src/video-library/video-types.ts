import {
  Clapperboard,
  Film,
  GraduationCap,
  Lock,
  Mic2,
  MonitorPlay,
  Newspaper,
  Star,
  Tv2,
} from "lucide-react";
import type { ComponentType } from "react";

export interface VideoTypeInfo {
  type: string;
  label: string;
  description: string;
  detailedDescription: string;
  icon: ComponentType<{
    className?: string;
    size?: number | string;
    "aria-hidden"?: boolean;
  }>;
  iconName: string;
  color: string;
  bgClass: string;
  textClass: string;
}

export const VIDEO_TYPES: VideoTypeInfo[] = [
  {
    type: "movie",
    label: "media.libraryEditor.types.movie.label",
    description: "media.libraryEditor.types.movie.description",
    detailedDescription: "media.libraryEditor.types.movie.detailedDescription",
    icon: Film,
    iconName: "film",
    color: "#3b82f6",
    bgClass: "bg-blue-500/10 dark:bg-blue-500/15",
    textClass: "text-blue-600 dark:text-blue-400",
  },
  {
    type: "tv",
    label: "media.libraryEditor.types.tv.label",
    description: "media.libraryEditor.types.tv.description",
    detailedDescription: "media.libraryEditor.types.tv.detailedDescription",
    icon: Tv2,
    iconName: "tv-2",
    color: "#8b5cf6",
    bgClass: "bg-violet-500/10 dark:bg-violet-500/15",
    textClass: "text-violet-600 dark:text-violet-400",
  },
  {
    type: "anime",
    label: "media.libraryEditor.types.anime.label",
    description: "media.libraryEditor.types.anime.description",
    detailedDescription: "media.libraryEditor.types.anime.detailedDescription",
    icon: Clapperboard,
    iconName: "clapperboard",
    color: "#ec4899",
    bgClass: "bg-pink-500/10 dark:bg-pink-500/15",
    textClass: "text-pink-600 dark:text-pink-400",
  },
  {
    type: "documentary",
    label: "media.libraryEditor.types.documentary.label",
    description: "media.libraryEditor.types.documentary.description",
    detailedDescription:
      "media.libraryEditor.types.documentary.detailedDescription",
    icon: Newspaper,
    iconName: "newspaper",
    color: "#10b981",
    bgClass: "bg-emerald-500/10 dark:bg-emerald-500/15",
    textClass: "text-emerald-600 dark:text-emerald-400",
  },
  {
    type: "variety",
    label: "media.libraryEditor.types.variety.label",
    description: "media.libraryEditor.types.variety.description",
    detailedDescription:
      "media.libraryEditor.types.variety.detailedDescription",
    icon: Star,
    iconName: "star",
    color: "#f59e0b",
    bgClass: "bg-amber-500/10 dark:bg-amber-500/15",
    textClass: "text-amber-600 dark:text-amber-400",
  },
  {
    type: "concert",
    label: "media.libraryEditor.types.concert.label",
    description: "media.libraryEditor.types.concert.description",
    detailedDescription:
      "media.libraryEditor.types.concert.detailedDescription",
    icon: Mic2,
    iconName: "mic-2",
    color: "#f97316",
    bgClass: "bg-orange-500/10 dark:bg-orange-500/15",
    textClass: "text-orange-600 dark:text-orange-400",
  },
  {
    type: "online_video",
    label: "media.libraryEditor.types.online_video.label",
    description: "media.libraryEditor.types.online_video.description",
    detailedDescription:
      "media.libraryEditor.types.online_video.detailedDescription",
    icon: MonitorPlay,
    iconName: "monitor-play",
    color: "#ef4444",
    bgClass: "bg-red-500/10 dark:bg-red-500/15",
    textClass: "text-red-600 dark:text-red-400",
  },
  {
    type: "online_course",
    label: "media.libraryEditor.types.online_course.label",
    description: "media.libraryEditor.types.online_course.description",
    detailedDescription:
      "media.libraryEditor.types.online_course.detailedDescription",
    icon: GraduationCap,
    iconName: "graduation-cap",
    color: "#06b6d4",
    bgClass: "bg-cyan-500/10 dark:bg-cyan-500/15",
    textClass: "text-cyan-600 dark:text-cyan-400",
  },
  {
    type: "adult",
    label: "media.libraryEditor.types.adult.label",
    description: "media.libraryEditor.types.adult.description",
    detailedDescription: "media.libraryEditor.types.adult.detailedDescription",
    icon: Lock,
    iconName: "lock",
    color: "#6b7280",
    bgClass: "bg-gray-500/10 dark:bg-gray-500/15",
    textClass: "text-gray-600 dark:text-gray-400",
  },
];

const VIDEO_TYPE_MAP = new Map(VIDEO_TYPES.map((t) => [t.type, t]));

export function getVideoTypeInfo(type: string): VideoTypeInfo {
  return VIDEO_TYPE_MAP.get(type) ?? VIDEO_TYPES[0];
}
