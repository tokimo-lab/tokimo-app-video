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

export interface OnlineMediaAnalyzeProvider {
  id: string;
  name: string;
  displayName?: string;
  supportedContentTypes: string[];
  requiresAuth: boolean;
}

export interface OnlineMediaProvider {
  id: string;
  name: string;
  displayName: string;
  sourceSite: string;
  supportedContentTypes: string[];
  requiresAuth: boolean;
  authConfigurable: boolean;
  commonSourceSites: string[];
  sourceSiteAliases: string[];
  hostSuffixes: string[];
}

export interface OnlineMediaCapability {
  canAnalyze: boolean;
  canDownload: boolean;
  canImportMetadata: boolean;
  supportsCollections: boolean;
}

export interface OnlineMediaAnalyzeResult {
  isSupported: boolean;
  provider?: OnlineMediaAnalyzeProvider | null;
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
  provider: OnlineMediaAnalyzeProvider | null;
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

export interface YtdlpStatus {
  installed: boolean;
  path: string;
  version?: string | null;
  latestVersion?: string | null;
}

export interface YtdlpUpdateResult {
  version: string;
}

export interface OnlineMediaProvidersResponse {
  providers: OnlineMediaProvider[];
  ytdlpAvailable: boolean;
}

export interface OnlineMediaAuthSetting {
  providerId: string;
  displayName: string;
  requiresAuth: boolean;
  cookieMasked?: string | null;
  isEnabled: boolean;
  updatedAt?: string | null;
}

export interface UpdateAuthSettingInput {
  displayName?: string;
  cookie?: string | null;
  isEnabled?: boolean;
}

// null/省略 = 保持原值，"" = 清空，"xxx" = 覆盖
export interface UpdateAuthSettingRequest {
  displayName?: string;
  cookie?: string | null;
  isEnabled?: boolean;
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

export interface VfsDisplayHints {
  protocolPrefix?: string | null;
  rootPath?: string | null;
}

export interface VfsDto {
  id: string;
  name: string;
  type: string;
  displayHints?: VfsDisplayHints | null;
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

const TMDB_GENRE_KEYS: Record<number, string> = {
  12: "media.genres.adventure",
  14: "media.genres.fantasy",
  16: "media.genres.animation",
  18: "media.genres.drama",
  27: "media.genres.horror",
  28: "media.genres.action",
  35: "media.genres.comedy",
  36: "media.genres.history",
  37: "media.genres.western",
  53: "media.genres.thriller",
  80: "media.genres.crime",
  99: "media.genres.documentary",
  878: "media.genres.scienceFiction",
  9648: "media.genres.mystery",
  10402: "media.genres.music",
  10749: "media.genres.romance",
  10751: "media.genres.family",
  10752: "media.genres.war",
  10759: "media.genres.actionAdventure",
  10762: "media.genres.kids",
  10763: "media.genres.news",
  10764: "media.genres.reality",
  10765: "media.genres.scifiFantasy",
  10766: "media.genres.soap",
  10767: "media.genres.talk",
  10768: "media.genres.warPolitics",
  10770: "media.genres.tvMovie",
};

export function getGenreKey(tmdbGenreId: number): string {
  return TMDB_GENRE_KEYS[tmdbGenreId] ?? "";
}
