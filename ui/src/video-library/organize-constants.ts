/**
 * 整理功能常量 & 默认模板
 * 从 MediaFoldersPage 抽离，供组件和预览复用。
 */

// ─── Organize Language Options ───

export const ORGANIZE_LANG_OPTIONS = [
  { value: "zh-CN", label: "media.organize.constants.languages.zhCN" },
  { value: "en-US", label: "media.organize.constants.languages.enUS" },
  { value: "ja-JP", label: "media.organize.constants.languages.jaJP" },
];

// ─── Template Variable Definitions ───

export type PlaceholderItem = { key: string; descKey: string };

export const MOVIE_VARS: PlaceholderItem[] = [
  { key: "name", descKey: "movieName" },
  { key: "year", descKey: "year" },
  { key: "category", descKey: "category" },
  { key: "country", descKey: "country" },
  { key: "language", descKey: "language" },
  { key: "version", descKey: "version" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "audio", descKey: "audio" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const TV_VARS: PlaceholderItem[] = [
  { key: "name", descKey: "tvName" },
  { key: "year", descKey: "year" },
  { key: "season", descKey: "season" },
  { key: "ep_start", descKey: "epStart" },
  { key: "ep_end", descKey: "epEnd" },
  { key: "category", descKey: "category" },
  { key: "country", descKey: "country" },
  { key: "language", descKey: "language" },
  { key: "version", descKey: "version" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "audio", descKey: "audio" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const ADULT_VARS: PlaceholderItem[] = [
  { key: "video_id", descKey: "videoId" },
  { key: "series", descKey: "series" },
  { key: "actress", descKey: "actress" },
  { key: "actresses", descKey: "actresses" },
  { key: "studio", descKey: "studio" },
  { key: "name", descKey: "adultName" },
  { key: "year", descKey: "year" },
  { key: "category", descKey: "category" },
  { key: "country", descKey: "country" },
  { key: "language", descKey: "language" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const MUSIC_VARS: PlaceholderItem[] = [
  { key: "artist", descKey: "artist" },
  { key: "album", descKey: "album" },
  { key: "title", descKey: "musicTitle" },
  { key: "track", descKey: "track" },
  { key: "disc", descKey: "disc" },
  { key: "genre", descKey: "genre" },
  { key: "year", descKey: "year" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const ONLINE_VIDEO_VARS: PlaceholderItem[] = [
  { key: "title", descKey: "onlineVideoTitle" },
  { key: "source_site", descKey: "sourceSite" },
  { key: "source_id", descKey: "sourceId" },
  { key: "provider_id", descKey: "providerId" },
  { key: "uploader", descKey: "uploader" },
  { key: "upload_date", descKey: "uploadDate" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "audio", descKey: "audio" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const AUDIOBOOK_VARS: PlaceholderItem[] = [
  { key: "author", descKey: "artist" },
  { key: "title", descKey: "musicTitle" },
  { key: "narrator", descKey: "artist" },
  { key: "year", descKey: "year" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "source", descKey: "source" },
  { key: "group", descKey: "group" },
];

export const PODCAST_VARS: PlaceholderItem[] = [
  { key: "show", descKey: "tvName" },
  { key: "title", descKey: "musicTitle" },
  { key: "episode", descKey: "epStart" },
  { key: "year", descKey: "year" },
  { key: "quality", descKey: "quality" },
  { key: "codec", descKey: "codec" },
  { key: "source", descKey: "source" },
];

export const EBOOK_VARS: PlaceholderItem[] = [
  { key: "author", descKey: "artist" },
  { key: "title", descKey: "musicTitle" },
  { key: "year", descKey: "year" },
  { key: "quality", descKey: "quality" },
  { key: "group", descKey: "group" },
];

export const BOOK_VARS: PlaceholderItem[] = [
  { key: "author", descKey: "artist" },
  { key: "title", descKey: "musicTitle" },
  { key: "volume", descKey: "epStart" },
  { key: "year", descKey: "year" },
  { key: "group", descKey: "group" },
];

export const MANGA_VARS: PlaceholderItem[] = [
  { key: "author", descKey: "artist" },
  { key: "title", descKey: "musicTitle" },
  { key: "volume", descKey: "epStart" },
  { key: "chapter", descKey: "epStart" },
  { key: "year", descKey: "year" },
  { key: "group", descKey: "group" },
];

export const DOCUMENT_VARS: PlaceholderItem[] = [
  { key: "author", descKey: "artist" },
  { key: "title", descKey: "musicTitle" },
  { key: "category", descKey: "group" },
  { key: "year", descKey: "year" },
];

export const PHOTO_VARS: PlaceholderItem[] = [
  { key: "album", descKey: "album" },
  { key: "date", descKey: "year" },
  { key: "camera", descKey: "codec" },
];

export function getVarsForType(ct: string): PlaceholderItem[] {
  switch (ct) {
    case "movie":
    case "documentary":
      return MOVIE_VARS;
    case "tv":
    case "anime":
    case "variety":
      return TV_VARS;
    case "adult":
      return ADULT_VARS;
    case "music":
      return MUSIC_VARS;
    case "audiobook":
      return AUDIOBOOK_VARS;
    case "podcast":
      return PODCAST_VARS;
    case "ebook":
      return EBOOK_VARS;
    case "book":
      return BOOK_VARS;
    case "manga":
      return MANGA_VARS;
    case "docs":
      return DOCUMENT_VARS;
    case "photo":
      return PHOTO_VARS;
    case "online_video":
      return ONLINE_VIDEO_VARS;
    default:
      return MOVIE_VARS;
  }
}

// Re-export media organize defaults for compatibility
export {
  getDefaultFileFormat,
  getDefaultFolderFormat,
} from "../lib/media-organize";

// ─── Template Renderer (preview) ───

/** Minimal Jinja2-like template renderer for preview (supports {{actresses(N)}}) */
export function renderTemplate(
  tpl: string,
  vars: Record<string, string>,
): string {
  if (!tpl) return "";
  let result = tpl.replace(
    /\{%\s*if\s+(\w+)\s*%\}([\s\S]*?)\{%\s*endif\s*%\}/g,
    (_, varName: string, body: string) => {
      const val = vars[varName] ?? "";
      return val ? body : "";
    },
  );
  result = result.replace(
    /\{\{actresses\((\d+)\)\}\}/g,
    (_, maxStr: string) => {
      const all = vars.actresses ?? "";
      if (!all) return "";
      const parts = all.split(", ");
      const max = Number.parseInt(maxStr, 10);
      if (parts.length > max) return `${parts.slice(0, max).join(", ")}, ...`;
      return all;
    },
  );
  result = result.replace(
    /\{\{(\w+)\}\}/g,
    (_, varName: string) => vars[varName] ?? "",
  );
  return result;
}
