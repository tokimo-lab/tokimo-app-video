/**
 * 整理预览示例数据（多语言）
 *
 * 每种内容类型 × 每种刮削语言一套示例。
 * - 技术字段（quality/codec/audio/source/group）语言无关，共享同一份。
 * - category 字段由外部注入（从后端 REGION_CATEGORIES 表获取），此处不包含。
 * - Adult 演员/厂牌为日文专有名词，所有语言保持不变。
 */

// ─── 通用技术参数（语言无关） ───

const TECH_MOVIE = {
  version: "2160p h265 atmos ExGrp",
  quality: "2160p",
  codec: "h265",
  audio: "atmos",
  source: "WEB-DL",
  group: "ExGrp",
};

const TECH_TV = {
  version: "2160p h265 aac ExGrp",
  quality: "2160p",
  codec: "h265",
  audio: "aac",
  source: "WEB-DL",
  group: "ExGrp",
};

const TECH_ONLINE_VIDEO = {
  quality: "1080p",
  codec: "h264",
  audio: "aac",
  source: "WEB-DL",
  group: "ExGrp",
};

const TECH_ADULT = {
  quality: "1080p",
  codec: "h264",
  source: "WEB-DL",
  group: "ExGrp",
};

// ─── 多语言样本名称 ───

type LangSamples = Record<string, Record<string, string>>;

const MOVIE_NAMES: LangSamples = {
  "zh-CN": {
    name: "media.organize.samples.movieName",
    country: "US",
    language: "en",
  },
  "en-US": { name: "Movie Name", country: "US", language: "en" },
  "ja-JP": { name: "映画名", country: "US", language: "en" },
};

const TV_NAMES: LangSamples = {
  "zh-CN": {
    name: "media.organize.samples.showName",
    country: "CN",
    language: "zh",
  },
  "en-US": { name: "Show Name", country: "CN", language: "zh" },
  "ja-JP": { name: "ドラマ名", country: "CN", language: "zh" },
};

const ADULT_NAMES: LangSamples = {
  "zh-CN": { name: "media.organize.samples.adultName" },
  "en-US": { name: "Newcomer NO.1STYLE" },
  "ja-JP": { name: "新人NO.1STYLE" },
};

const MUSIC_NAMES: LangSamples = {
  "zh-CN": {
    artist: "media.organize.samples.artistName",
    album: "media.organize.samples.albumName",
    title: "media.organize.samples.trackTitle",
  },
  "en-US": { artist: "Artist Name", album: "Album Name", title: "Track Title" },
  "ja-JP": { artist: "アーティスト名", album: "アルバム名", title: "曲名" },
};

const ONLINE_VIDEO_NAMES: LangSamples = {
  "zh-CN": {
    title: "media.organize.samples.onlineVideoTitle",
    source_site: "Bilibili",
    provider_id: "bilibili",
    uploader: "media.organize.samples.sampleUploader",
  },
  "en-US": {
    title: "Ringtone Era Was Peak Content",
    source_site: "Bilibili",
    provider_id: "bilibili",
    uploader: "Sample Creator",
  },
  "ja-JP": {
    title: "着信音時代は神コンテンツだった",
    source_site: "Bilibili",
    provider_id: "bilibili",
    uploader: "サンプル投稿者",
  },
};

// ─── 固定字段 ───

const MOVIE_FIXED = { year: "2019" };
const TV_FIXED = { year: "2023", season: "01", ep_start: "01", ep_end: "" };
const ADULT_FIXED = {
  video_id: "SSIS-001",
  series: "SSIS",
  actress: "三上悠亜",
  actresses: "三上悠亜, 橋本有菜, 小島南",
  studio: "S1 NO.1 STYLE",
  year: "2021",
  country: "JP",
  language: "ja",
};
const MUSIC_FIXED = {
  track: "01",
  disc: "1",
  genre: "Pop",
  year: "2024",
  quality: "FLAC",
  codec: "FLAC",
  source: "CD",
  group: "",
};

const ONLINE_VIDEO_FIXED = {
  source_id: "BV1YjPpzyEtd",
  upload_date: "2026-03-17",
};

// ─── 默认 fallback（未匹配的刮削语言用 zh-CN） ───

const DEFAULT_LANG = "zh-CN";

function pick(map: LangSamples, lang: string): Record<string, string> {
  return map[lang] || map[DEFAULT_LANG];
}

// ─── 公开 API ───

/**
 * 获取指定内容类型 + 刮削语言的示例数据。
 * 返回值不含 category 字段，调用方需从 regionCategories map 注入。
 */
export function getOrganizeSample(
  contentType: string,
  organizeLang: string,
): Record<string, string> {
  switch (contentType) {
    case "movie":
    case "documentary":
      return {
        ...MOVIE_FIXED,
        ...pick(MOVIE_NAMES, organizeLang),
        ...TECH_MOVIE,
      };
    case "tv":
    case "anime":
    case "variety":
      return {
        ...TV_FIXED,
        ...pick(TV_NAMES, organizeLang),
        ...TECH_TV,
      };
    case "adult":
      return {
        ...ADULT_FIXED,
        ...pick(ADULT_NAMES, organizeLang),
        ...TECH_ADULT,
      };
    case "music":
      return {
        ...MUSIC_FIXED,
        ...pick(MUSIC_NAMES, organizeLang),
      };
    case "audiobook":
      return {
        author: pick(MUSIC_NAMES, organizeLang).artist || "Author",
        title: pick(MUSIC_NAMES, organizeLang).title || "Title",
        narrator: pick(MUSIC_NAMES, organizeLang).artist || "Narrator",
        year: "2024",
        quality: "320kbps",
        codec: "mp3",
        source: "Audible",
        group: "",
      };
    case "podcast":
      return {
        show: pick(TV_NAMES, organizeLang).name || "Show",
        title: pick(MUSIC_NAMES, organizeLang).title || "Episode Title",
        episode: "042",
        year: "2024",
        quality: "128kbps",
        codec: "mp3",
        source: "RSS",
      };
    case "ebook":
      return {
        author: pick(MUSIC_NAMES, organizeLang).artist || "Author",
        title: pick(MOVIE_NAMES, organizeLang).name || "Book Title",
        year: "2024",
        quality: "EPUB",
        group: "",
      };
    case "book":
      return {
        author: pick(MUSIC_NAMES, organizeLang).artist || "Author",
        title: pick(MOVIE_NAMES, organizeLang).name || "Book Title",
        volume: "03",
        year: "2024",
        group: "",
      };
    case "manga":
      return {
        author: pick(MUSIC_NAMES, organizeLang).artist || "Author",
        title: pick(MOVIE_NAMES, organizeLang).name || "Manga Title",
        volume: "05",
        chapter: "042",
        year: "2024",
        group: "ScanGroup",
      };
    case "docs":
      return {
        author: pick(MUSIC_NAMES, organizeLang).artist || "Author",
        title: pick(MOVIE_NAMES, organizeLang).name || "Document Title",
        category: "Technical",
        year: "2024",
      };
    case "photo":
      return {
        album: pick(MUSIC_NAMES, organizeLang).album || "Album",
        date: "2024-03-22",
        camera: "Sony A7IV",
      };
    case "online_video":
      return {
        ...ONLINE_VIDEO_FIXED,
        ...pick(ONLINE_VIDEO_NAMES, organizeLang),
        ...TECH_ONLINE_VIDEO,
      };
    default:
      return {
        ...MOVIE_FIXED,
        ...pick(MOVIE_NAMES, organizeLang),
        ...TECH_MOVIE,
      };
  }
}
