/**
 * Self-contained type definitions for the video app's API client.
 * Copied from the host shell so the standalone bundle has no @/* dependencies.
 */

// ── JSON helpers ────────────────────────────────────────────────────────────

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

// ── App types (from packages/web/src/types/app.ts) ──────────────────────────

export type AppType =
  | "movie"
  | "tv"
  | "anime"
  | "documentary"
  | "variety"
  | "concert"
  | "online_course"
  | "music"
  | "audiobook"
  | "podcast"
  | "book"
  | "manga"
  | "ebook"
  | "docs"
  | "online_video"
  | "photo"
  | "adult";

export interface VideoItemOutput {
  id: string;
  appId: string;
  title: string;
  originalTitle?: string | null;
  year?: number | null;
  releaseDate?: string | null;
  posterPath?: string | null;
  backdropPath?: string | null;
  overview?: string | null;
  rating?: number | null;
  isAdult: boolean;
  isFavorite?: boolean;
  scrapedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface TvShowOutput {
  id: string;
  appId: string;
  title: string;
  originalTitle?: string | null;
  year?: number | null;
  firstAirDate?: string | null;
  posterPath?: string | null;
  backdropPath?: string | null;
  overview?: string | null;
  rating?: number | null;
  isFavorite?: boolean;
  status?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface PersonOutput {
  id: string;
  name: string;
  originalName?: string | null;
  profilePath?: string | null;
}

export interface CreditOutput {
  id: string;
  role: string;
  character?: string | null;
  sortOrder: number;
  person: PersonOutput;
}

export interface GenreOutput {
  id: string;
  tmdbGenreId: number;
  name: string;
}

export interface SubtitleOutput {
  id: string;
  language: string;
  title?: string | null;
  sourceType: string;
  format: string;
  isDefault: boolean;
  isForced: boolean;
  isHearingImpaired: boolean;
  streamIndex?: number | null;
  storageUrl?: string | null;
  source?: string | null;
  createdAt?: string;
}

export interface ChapterOutput {
  id: string;
  index: number;
  title?: string | null;
  startTime: number;
  endTime?: number | null;
  thumbPath?: string | null;
}

export interface MediaFileOutput {
  id: string;
  path: string;
  filename: string;
  streamKey?: string | null;
  size?: number | null;
  mimeType?: string | null;
  duration?: number | null;
  checksum?: string | null;
  videoCodec?: string | null;
  videoWidth?: number | null;
  videoHeight?: number | null;
  videoProfile?: string | null;
  hdrType?: string | null;
  videoStreams?: unknown | null;
  audioStreams?: unknown | null;
  ffprobeRaw?: unknown | null;
  sourceName?: string | null;
  sourceType?: string | null;
  sourceAddress?: string | null;
  isAvailable?: boolean;
  subtitles?: SubtitleOutput[];
  chapters?: ChapterOutput[];
  scannedAt?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export interface CollectionOutput {
  id: string;
  name: string;
  posterPath?: string | null;
  overview?: string | null;
}

export interface PersonDetailOutput {
  id: string;
  name: string;
  originalName?: string | null;
  profilePath?: string | null;
  profileKey?: string | null;
  biography?: string | null;
  birthday?: string | null;
  deathday?: string | null;
  birthplace?: string | null;
  gender?: string | null;
  aliases?: string[];
  tmdbId?: string | null;
  imdbId?: string | null;
  knownForDepartment?: string | null;
  popularity?: number | null;
  credits?: {
    id: string;
    role: string;
    character?: string | null;
    sortOrder: number;
    videoItemId?: string | null;
    tvShowId?: string | null;
    appId?: string | null;
    mediaTitle?: string | null;
    mediaYear?: number | null;
    mediaPosterPath?: string | null;
  }[];
}

export interface VideoItemDetailOutput extends VideoItemOutput {
  sortTitle?: string | null;
  runtime?: number | null;
  tagline?: string | null;
  contentRating?: string | null;
  countries?: string[];
  tmdbId?: string | null;
  imdbId?: string | null;
  tmdbRating?: number | null;
  imdbRating?: number | null;
  doubanRating?: number | null;
  genres?: GenreOutput[];
  credits?: CreditOutput[];
  files?: MediaFileOutput[];
  collections?: CollectionOutput[];
  metadata?: {
    uploader?: string;
    sourceSite?: string;
    sourceUrl?: string;
    externalId?: string;
    durationSeconds?: number;
  } | null;
}

export interface EpisodeOutput {
  id: string;
  episodeNumber: number;
  title?: string | null;
  overview?: string | null;
  airDate?: string | null;
  runtime?: number | null;
  stillPath?: string | null;
  rating?: number | null;
  files?: MediaFileOutput[];
}

export interface SeasonOutput {
  id: string;
  seasonNumber: number;
  title?: string | null;
  overview?: string | null;
  airDate?: string | null;
  posterPath?: string | null;
  episodeCount?: number | null;
  episodes?: EpisodeOutput[];
}

export interface TvShowDetailOutput extends TvShowOutput {
  sortTitle?: string | null;
  lastAirDate?: string | null;
  contentRating?: string | null;
  countries?: string[];
  tmdbId?: string | null;
  imdbId?: string | null;
  tvdbId?: string | null;
  tmdbRating?: number | null;
  imdbRating?: number | null;
  doubanRating?: number | null;
  genres?: GenreOutput[];
  credits?: CreditOutput[];
  seasons?: SeasonOutput[];
  collections?: CollectionOutput[];
}

// ── Search / TMDB types (from packages/web/src/types/search.ts) ─────────────

export interface TmdbMedia {
  id: number;
  mediaType: "movie" | "tv";
  title: string;
  originalTitle?: string;
  overview?: string;
  posterPath?: string | null;
  backdropPath?: string | null;
  releaseDate?: string;
  voteAverage?: number;
  voteCount?: number;
  popularity?: number;
  originalLanguage?: string;
  genreIds?: number[];
  imdbId?: string | null;
  source?: "tmdb" | "imdb" | "both" | "douban";
  imdbRating?: number;
  totalSeasons?: number;
  doubanId?: string | null;
  doubanRating?: number;
}

export interface TmdbMediaDetail extends TmdbMedia {
  runtime?: number;
  status?: string;
  tagline?: string;
  budget?: number;
  revenue?: number;
  homepage?: string;
  numberOfSeasons?: number;
  numberOfEpisodes?: number;
  originCountry?: string[];
  genres?: { id: number; name: string }[];
  productionCompanies?: {
    id: number;
    name: string;
    logoPath?: string | null;
  }[];
  cast?: {
    name: string;
    role?: string;
    tmdbId?: number;
    thumb?: string;
  }[];
}

// ── Adult metadata (from packages/web/src/types/adult-metadata.ts) ──────────

export type AdultMetadataSource = "javbus" | "javdb" | "tpdb" | "stashdb";

export interface AdultMetadata {
  videoId: string;
  title?: string;
  posterUrl?: string;
  coverUrl?: string;
  sourceUrl?: string;
  actors?: string[];
  genres?: string[];
  releaseDate?: string;
  studio?: string;
  duration?: number;
  rating?: number;
  source: AdultMetadataSource;
}

// ── Media-organize (from packages/web/src/types/media-organize.ts) ──────────

export type ContentType =
  | "movie"
  | "tv"
  | "anime"
  | "documentary"
  | "variety"
  | "concert"
  | "online_course"
  | "music"
  | "audiobook"
  | "podcast"
  | "book"
  | "manga"
  | "ebook"
  | "docs"
  | "online_video"
  | "photo"
  | "adult";

export type LinkMode = "hardlink" | "softlink" | "copy" | "move";

export interface OrganizeSettings {
  linkMode?: LinkMode;
  folderFormat?: string | null;
  fileFormat?: string | null;
  organizeLang?: string | null;
  flattenDisc?: boolean;
  fixEmbyDisc?: boolean;
  strictYearMatch?: boolean;
  [key: string]: unknown;
}

export interface ParsedMediaInfo {
  title: string;
  year?: number | null;
  season?: number | null;
  episodes?: number[] | null;
  quality?: string | null;
  codec?: string | null;
  source?: string | null;
  audioCodec?: string | null;
  releaseGroup?: string | null;
  contentType: "movie" | "tv" | "adult" | "music" | "unknown";
  artist?: string | null;
  albumArtist?: string | null;
  album?: string | null;
  trackTitle?: string | null;
  trackNumber?: number | null;
  discNumber?: number | null;
  genre?: string | null;
  musicYear?: number | null;
}

export type TmdbMatchStatus = "unmatched" | "matched" | "multiple" | "failed";

export interface TmdbMatchResult {
  status: TmdbMatchStatus;
  candidates: TmdbMedia[];
  selectedId?: number | null;
  selectedDetail?: TmdbMediaDetail | null;
}

export type MusicMatchStatus = "unmatched" | "matched" | "multiple" | "failed";

export interface MusicMatchCandidate {
  mbReleaseId: string;
  title: string;
  artist: string;
  year?: number | null;
  trackCount?: number | null;
  country?: string | null;
  format?: string | null;
  score?: number | null;
}

export interface MusicMatchDetail {
  mbReleaseId: string;
  mbReleaseGroupId?: string | null;
  title: string;
  artist: string;
  artistMbId?: string | null;
  year?: number | null;
  releaseDate?: string | null;
  albumType?: string | null;
  genres?: string[] | null;
  totalTracks?: number | null;
  totalDiscs?: number | null;
  coverUrl?: string | null;
  overview?: string | null;
  tracks?: { number: number; title: string; duration?: number | null }[] | null;
}

export interface MusicMatchResult {
  status: MusicMatchStatus;
  candidates: MusicMatchCandidate[];
  selected?: MusicMatchDetail | null;
}

export type OrganizeItemStatus =
  | "pending"
  | "identified"
  | "ready"
  | "organizing"
  | "success"
  | "organized"
  | "failed"
  | "skipped";

export interface OrganizeItem {
  id: string;
  sourcePath: string;
  fileName: string;
  parentDir?: string | null;
  isDirectory: boolean;
  children?: OrganizeItem[] | null;
  parsed: ParsedMediaInfo;
  tmdbMatch: TmdbMatchResult;
  targetAppId?: string | null;
  targetPath?: string | null;
  linkMode: LinkMode;
  itemStatus: OrganizeItemStatus;
  error?: string | null;
  fileSize?: number | null;
  isDisc?: boolean | null;
  adultMatch?: AdultMetadata | null;
  musicMatch?: MusicMatchResult | null;
}

export type OrganizeSessionStatus =
  | "idle"
  | "scanning"
  | "scanned"
  | "identifying"
  | "identified"
  | "executing"
  | "done";

export interface NfoInfo {
  tmdbUrl?: string;
  nfoPath?: string;
  nfoSummary?: string;
  artworkPaths?: string[];
  artworkLogs?: string[];
  thumbPath?: string;
}

export interface OrganizeReportItem {
  itemId: string;
  fileName: string;
  status: "success" | "failed" | "skipped";
  sourcePath: string;
  targetPath?: string | null;
  linkMode?: LinkMode | null;
  error?: string | null;
  nfoInfo?: NfoInfo;
}

export interface OrganizeReport {
  totalItems: number;
  successCount: number;
  failedCount: number;
  skippedCount: number;
  results: OrganizeReportItem[];
}

export interface SavedOrganizeReport {
  id: string;
  sourcePath: string;
  totalItems: number;
  successCount: number;
  failedCount: number;
  skippedCount: number;
  results: OrganizeReportItem[];
  mediaNames: string[];
  createdAt: string;
}

export interface OrganizeReportSummary {
  id: string;
  sourcePath: string;
  totalItems: number;
  successCount: number;
  failedCount: number;
  skippedCount: number;
  mediaNames: string[];
  createdAt: string;
}

export interface OrganizeSession {
  id: string;
  status: OrganizeSessionStatus;
  sourcePath: string;
  sourceId?: string | null;
  items: OrganizeItem[];
  progress?: { current: number; total: number; currentFile?: string } | null;
  report?: OrganizeReport | null;
  createdAt: string;
  updatedAt: string;
}

// ── Online media (from packages/web/src/types/online-media.ts) ──────────────

export interface OnlineMediaProvider {
  id: string;
  name: string;
  displayName?: string;
  supportedContentTypes: ContentType[];
  requiresAuth: boolean;
}

export interface OnlineMediaCapability {
  canAnalyze: boolean;
  canDownload: boolean;
  canImportMetadata: boolean;
  supportsCollections: boolean;
}

export interface OnlineMediaAnalyzeResult {
  isSupported: boolean;
  provider?: OnlineMediaProvider | null;
  capability?: OnlineMediaCapability | null;
  sourceSite?: string | null;
  sourceId?: string | null;
  normalizedUrl?: string | null;
  title?: string | null;
  description?: string | null;
  thumbnailUrl?: string | null;
  durationSeconds?: number | null;
  uploader?: string | null;
  externalId?: string | null;
  contentType?: ContentType | null;
  requiresAuth: boolean;
  warnings: string[];
  rawMetadata?: Record<string, unknown> | null;
  artist?: string | null;
  albumArtist?: string | null;
  album?: string | null;
  trackTitle?: string | null;
  trackNumber?: number | null;
  discNumber?: number | null;
  genre?: string | null;
  releaseDate?: string | null;
}

export type OnlineMediaDownloadFormat = "auto" | "audio_only" | "video";

export interface StartOnlineMediaDownloadInput {
  url: string;
  targetAppId: string;
  mediaTitle?: string;
  mediaYear?: string;
  autoOrganize?: boolean;
  confirmDuplicate?: boolean;
  existingRecordId?: string;
  downloadFormat?: OnlineMediaDownloadFormat;
  analysis: OnlineMediaAnalyzeResult;
}

export interface StartOnlineMediaDownloadStartedOutput {
  action: "started" | "restarted";
  recordId: string;
  jobId: string;
}

export interface StartOnlineMediaDownloadDuplicateOutput {
  action: "duplicate";
  existingRecordId: string;
  existingStatus: string;
  existingTitle?: string | null;
  existingSourceSite?: string | null;
  existingSourceUrl?: string | null;
  message: string;
}

export type StartOnlineMediaDownloadOutput =
  | StartOnlineMediaDownloadStartedOutput
  | StartOnlineMediaDownloadDuplicateOutput;

// ── Online media API request/response shapes ────────────────────────────────

export interface AnalyzeOnlineMediaRequest {
  url: string;
  targetAppId?: string;
  preferredProvider?: string;
}

export interface AnalyzeOnlineMediaResponse {
  isSupported: boolean;
  provider: OnlineMediaProvider | null;
  capability: OnlineMediaCapability | null;
  sourceSite: string | null;
  sourceId: string | null;
  normalizedUrl: string | null;
  title: string | null;
  description: string | null;
  thumbnailUrl: string | null;
  durationSeconds: number | null;
  uploader: string | null;
  artist: string | null;
  albumArtist: string | null;
  album: string | null;
  trackTitle: string | null;
  trackNumber: number | null;
  discNumber: number | null;
  genre: string | null;
  releaseDate: string | null;
  externalId: string | null;
  contentType: string | null;
  requiresAuth: boolean;
  warnings: string[];
  rawMetadata: unknown;
}

// ── Generated rust-types ────────────────────────────────────────────────────

export interface VideoSourceOutput {
  sourceId: string;
  rootPath: string;
  sortOrder: number;
  isDefaultDownload: boolean;
  sourceName: string | null;
  sourceType: string | null;
}

export interface VideoOutput {
  id: string;
  name: string;
  type: string;
  avatar: JsonValue | null;
  description: string | null;
  posterPath: string | null;
  scrapeEnabled: boolean;
  scrapeAgents: Array<string> | null;
  sortOrder: number;
  settings: JsonValue | null;
  syncStatus: string;
  lastSyncAt: string | null;
  itemCount: number;
  sources: Array<VideoSourceOutput>;
  createdAt: string;
  updatedAt: string;
}

export interface VfsDto {
  id: string;
  name: string;
  type: string;
  config: JsonValue | null;
  sortOrder: number;
  lastScanAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface FileProbeStream {
  index: number;
  codecType: string;
  codecName: string;
  codecLongName: string;
  profile: string | null;
  width: number | null;
  height: number | null;
  displayAspectRatio: string | null;
  pixFmt: string | null;
  colorSpace: string | null;
  colorTransfer: string | null;
  colorPrimaries: string | null;
  colorRange: string | null;
  fieldOrder: string | null;
  frameRate: string | null;
  sampleRate: number | null;
  channels: number | null;
  channelLayout: string | null;
  duration: string | null;
  bitRate: string | null;
  tags: { [key in string]: string };
}

export interface FileProbeChapter {
  id: number;
  startTime: string;
  endTime: string;
  title: string | null;
}

export interface FileProbeFormat {
  formatName: string;
  formatLongName: string;
  nbStreams: number;
  duration: number | null;
  size: number | null;
  bitRate: number | null;
  tags: { [key in string]: string };
}

export interface FileProbeResult {
  format: FileProbeFormat;
  streams: Array<FileProbeStream>;
  chapters: Array<FileProbeChapter>;
}

export interface VideoTaskProgress {
  taskType: string;
  status: string;
  totalItems: number;
  processedItems: number;
}

export interface VideoSyncProgressOutput {
  videoId: string;
  status: string;
  total: number;
  completed: number;
  running: number;
  pending: number;
  failed: number;
  tasks: Array<VideoTaskProgress>;
}

// ── Watch history ──────────────────────────────────────────────────────────

export interface WatchHistoryEntry {
  id: string;
  fileId: string | null;
  userName: string | null;
  clientName: string | null;
  userAgent: string | null;
  startedAt: string;
  stoppedAt: string | null;
  position: number;
  duration: number | null;
  completed: boolean;
  episodeId?: string | null;
  seasonNumber?: number | null;
  episodeNumber?: number | null;
}

// ── TMDB genre name resolver (from packages/types/src/tmdb-genres.ts) ───────

const TMDB_GENRE_NAMES: Record<string, Record<number, string>> = {
  en: {
    12: "Adventure",
    14: "Fantasy",
    16: "Animation",
    18: "Drama",
    27: "Horror",
    28: "Action",
    35: "Comedy",
    36: "History",
    37: "Western",
    53: "Thriller",
    80: "Crime",
    99: "Documentary",
    878: "Science Fiction",
    9648: "Mystery",
    10402: "Music",
    10749: "Romance",
    10751: "Family",
    10752: "War",
    10759: "Action & Adventure",
    10762: "Kids",
    10763: "News",
    10764: "Reality",
    10765: "Sci-Fi & Fantasy",
    10766: "Soap",
    10767: "Talk",
    10768: "War & Politics",
    10770: "TV Movie",
  },
  zh: {
    12: "冒险",
    14: "奇幻",
    16: "动画",
    18: "剧情",
    27: "恐怖",
    28: "动作",
    35: "喜剧",
    36: "历史",
    37: "西部",
    53: "惊悚",
    80: "犯罪",
    99: "纪录",
    878: "科幻",
    9648: "悬疑",
    10402: "音乐",
    10749: "爱情",
    10751: "家庭",
    10752: "战争",
    10759: "动作冒险",
    10762: "儿童",
    10763: "新闻",
    10764: "真人秀",
    10765: "科幻奇幻",
    10766: "肥皂剧",
    10767: "脱口秀",
    10768: "战争政治",
    10770: "电视电影",
  },
  ja: {
    12: "アドベンチャー",
    14: "ファンタジー",
    16: "アニメーション",
    18: "ドラマ",
    27: "ホラー",
    28: "アクション",
    35: "コメディ",
    36: "歴史",
    37: "西部劇",
    53: "スリラー",
    80: "犯罪",
    99: "ドキュメンタリー",
    878: "サイエンスフィクション",
    9648: "ミステリー",
    10402: "音楽",
    10749: "ロマンス",
    10751: "ファミリー",
    10752: "戦争",
    10759: "アクション & アドベンチャー",
    10762: "キッズ",
    10763: "ニュース",
    10764: "リアリティ",
    10765: "SF & ファンタジー",
    10766: "ソープ",
    10767: "トーク",
    10768: "戦争と政治",
    10770: "テレビ映画",
  },
  de: {
    12: "Abenteuer",
    14: "Fantasy",
    16: "Animation",
    18: "Drama",
    27: "Horror",
    28: "Action",
    35: "Komödie",
    36: "Historie",
    37: "Western",
    53: "Thriller",
    80: "Krimi",
    99: "Dokumentarfilm",
    878: "Science Fiction",
    9648: "Krimi",
    10402: "Musik",
    10749: "Liebesfilm",
    10751: "Familie",
    10752: "Kriegsfilm",
    10759: "Action & Abenteuer",
    10762: "Kids",
    10763: "News",
    10764: "Reality",
    10765: "Sci-Fi & Fantasy",
    10766: "Soap",
    10767: "Talk",
    10768: "Krieg & Politik",
    10770: "TV-Film",
  },
};

export function getGenreName(tmdbGenreId: number, lang: string): string {
  const prefix = lang.split(/[-_]/)[0];
  return (
    TMDB_GENRE_NAMES[prefix]?.[tmdbGenreId] ??
    TMDB_GENRE_NAMES.en[tmdbGenreId] ??
    ""
  );
}
