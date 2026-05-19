/**
 * 整理功能常量 & 默认模板
 * 从 MediaFoldersPage 抽离，供组件和预览复用。
 */

// ─── Organize Language Options ───

export const ORGANIZE_LANG_OPTIONS = [
  { value: "zh-CN", label: "简体中文" },
  { value: "en-US", label: "English" },
  { value: "ja-JP", label: "日本語" },
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

// ─── Default Templates (must match server defaults) ───

const DEFAULT_MOVIE_FOLDER = "{{name}} ({{year}})";
const DEFAULT_MOVIE_FILE =
  "{{name}} ({{year}}){% if version %} - {{version}}{% endif %}";
const DEFAULT_TV_FOLDER = "{{name}} ({{year}})";
const DEFAULT_TV_FILE =
  "{{name}} S{{season}}E{{ep_start}}{% if ep_end %}-E{{ep_end}}{% endif %}{% if version %} - {{version}}{% endif %}";
const DEFAULT_ADULT_FOLDER = "{{series}}/{{video_id}}";
const DEFAULT_ADULT_FILE = "{{video_id}}";
const DEFAULT_MUSIC_FOLDER =
  "{{artist}}/{{album}}{% if year %} ({{year}}){% endif %}";
const DEFAULT_MUSIC_FILE = "{{track}}. {{title}}";
const DEFAULT_ONLINE_VIDEO_FOLDER = "{{source_site}}/{{source_id}}";
const DEFAULT_ONLINE_VIDEO_FILE =
  "{{title}}{% if source_id %} [{{source_id}}]{% endif %}";

const DEFAULT_AUDIOBOOK_FOLDER =
  "{{author}}/{{title}}{% if year %} ({{year}}){% endif %}";
const DEFAULT_AUDIOBOOK_FILE = "{{title}}";
const DEFAULT_PODCAST_FOLDER = "{{show}}";
const DEFAULT_PODCAST_FILE = "{{episode}}. {{title}}";
const DEFAULT_EBOOK_FOLDER = "{{author}}";
const DEFAULT_EBOOK_FILE = "{{title}}{% if year %} ({{year}}){% endif %}";
const DEFAULT_BOOK_FOLDER = "{{author}}/{{title}}";
const DEFAULT_BOOK_FILE = "{{title}}{% if volume %} Vol.{{volume}}{% endif %}";
const DEFAULT_MANGA_FOLDER = "{{author}}/{{title}}";
const DEFAULT_MANGA_FILE =
  "{{title}}{% if volume %} Vol.{{volume}}{% endif %}{% if chapter %} Ch.{{chapter}}{% endif %}";
const DEFAULT_DOCUMENT_FOLDER = "{% if category %}{{category}}/{% endif %}";
const DEFAULT_DOCUMENT_FILE = "{{title}}{% if year %} ({{year}}){% endif %}";
const DEFAULT_PHOTO_FOLDER = "{{album}}{% if date %}/{{date}}{% endif %}";
const DEFAULT_PHOTO_FILE = "{{date}}";

export function getDefaultFolderFormat(ct: string): string {
  switch (ct) {
    case "movie":
    case "documentary":
      return DEFAULT_MOVIE_FOLDER;
    case "tv":
    case "anime":
    case "variety":
      return DEFAULT_TV_FOLDER;
    case "adult":
      return DEFAULT_ADULT_FOLDER;
    case "music":
      return DEFAULT_MUSIC_FOLDER;
    case "audiobook":
      return DEFAULT_AUDIOBOOK_FOLDER;
    case "podcast":
      return DEFAULT_PODCAST_FOLDER;
    case "ebook":
      return DEFAULT_EBOOK_FOLDER;
    case "book":
      return DEFAULT_BOOK_FOLDER;
    case "manga":
      return DEFAULT_MANGA_FOLDER;
    case "docs":
      return DEFAULT_DOCUMENT_FOLDER;
    case "photo":
      return DEFAULT_PHOTO_FOLDER;
    case "online_video":
      return DEFAULT_ONLINE_VIDEO_FOLDER;
    default:
      return DEFAULT_MOVIE_FOLDER;
  }
}

export function getDefaultFileFormat(ct: string): string {
  switch (ct) {
    case "movie":
    case "documentary":
      return DEFAULT_MOVIE_FILE;
    case "tv":
    case "anime":
    case "variety":
      return DEFAULT_TV_FILE;
    case "adult":
      return DEFAULT_ADULT_FILE;
    case "music":
      return DEFAULT_MUSIC_FILE;
    case "audiobook":
      return DEFAULT_AUDIOBOOK_FILE;
    case "podcast":
      return DEFAULT_PODCAST_FILE;
    case "ebook":
      return DEFAULT_EBOOK_FILE;
    case "book":
      return DEFAULT_BOOK_FILE;
    case "manga":
      return DEFAULT_MANGA_FILE;
    case "docs":
      return DEFAULT_DOCUMENT_FILE;
    case "photo":
      return DEFAULT_PHOTO_FILE;
    case "online_video":
      return DEFAULT_ONLINE_VIDEO_FILE;
    default:
      return DEFAULT_MOVIE_FILE;
  }
}

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
