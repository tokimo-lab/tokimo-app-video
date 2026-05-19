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
    label: "电影",
    description: "电影长片、蓝光收藏",
    detailedDescription:
      "用于管理电影长片、纪录片、演唱会录像等单体视频作品。支持 TMDB 自动刮削封面、演员、简介，按年份 / 导演整理。",
    icon: Film,
    iconName: "film",
    color: "#3b82f6",
    bgClass: "bg-blue-500/10 dark:bg-blue-500/15",
    textClass: "text-blue-600 dark:text-blue-400",
  },
  {
    type: "tv",
    label: "剧集",
    description: "连续剧、网剧、迷你剧",
    detailedDescription:
      "用于管理连续剧、网剧、迷你剧等多集作品。支持 TMDB 季 / 集自动刮削，按剧名 / 季整理。",
    icon: Tv2,
    iconName: "tv-2",
    color: "#8b5cf6",
    bgClass: "bg-violet-500/10 dark:bg-violet-500/15",
    textClass: "text-violet-600 dark:text-violet-400",
  },
  {
    type: "anime",
    label: "动漫",
    description: "日漫、国漫、OVA",
    detailedDescription:
      "用于管理动画作品，包括剧场版、TV 动画、OVA 等。支持 Bangumi 刮削，可按年份 / 系列整理。",
    icon: Clapperboard,
    iconName: "clapperboard",
    color: "#ec4899",
    bgClass: "bg-pink-500/10 dark:bg-pink-500/15",
    textClass: "text-pink-600 dark:text-pink-400",
  },
  {
    type: "documentary",
    label: "纪录片",
    description: "纪录片、人文地理",
    detailedDescription:
      "用于管理纪录片，包括历史、自然、人文等题材。支持 TMDB 刮削，按主题 / 系列整理。",
    icon: Newspaper,
    iconName: "newspaper",
    color: "#10b981",
    bgClass: "bg-emerald-500/10 dark:bg-emerald-500/15",
    textClass: "text-emerald-600 dark:text-emerald-400",
  },
  {
    type: "variety",
    label: "综艺",
    description: "综艺节目、真人秀",
    detailedDescription:
      "用于管理综艺节目、真人秀等娱乐内容。按节目名 / 期数整理，支持多集管理。",
    icon: Star,
    iconName: "star",
    color: "#f59e0b",
    bgClass: "bg-amber-500/10 dark:bg-amber-500/15",
    textClass: "text-amber-600 dark:text-amber-400",
  },
  {
    type: "concert",
    label: "演唱会",
    description: "演唱会、音乐节录像",
    detailedDescription:
      "用于管理演唱会、音乐节、现场表演等录像内容。按艺人 / 年份整理。",
    icon: Mic2,
    iconName: "mic-2",
    color: "#f97316",
    bgClass: "bg-orange-500/10 dark:bg-orange-500/15",
    textClass: "text-orange-600 dark:text-orange-400",
  },
  {
    type: "online_video",
    label: "网络视频",
    description: "B 站、YouTube 等平台内容",
    detailedDescription:
      "用于管理从 B 站、YouTube 等平台下载的视频内容。按UP主 / 频道整理，保留原始元数据。",
    icon: MonitorPlay,
    iconName: "monitor-play",
    color: "#ef4444",
    bgClass: "bg-red-500/10 dark:bg-red-500/15",
    textClass: "text-red-600 dark:text-red-400",
  },
  {
    type: "online_course",
    label: "网课",
    description: "在线课程、教学视频",
    detailedDescription:
      "用于管理在线课程、教学视频、讲座录像等学习内容。按课程名 / 章节整理，支持进度追踪。",
    icon: GraduationCap,
    iconName: "graduation-cap",
    color: "#06b6d4",
    bgClass: "bg-cyan-500/10 dark:bg-cyan-500/15",
    textClass: "text-cyan-600 dark:text-cyan-400",
  },
  {
    type: "adult",
    label: "成人",
    description: "成人内容（需开启成人模式）",
    detailedDescription:
      "用于管理成人内容。需要在系统设置中开启成人模式。支持自定义整理规则。",
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
