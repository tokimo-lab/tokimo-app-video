import { HorizontalScroll, Image, Tag } from "@tokiomo/components";
import { getGenreName } from "@tokiomo/types";
import { Info, Play } from "lucide-react";
import { useState } from "react";
import FileDetailsModal, {
  getMediaFileLocator,
} from "@/apps/files/components/FileDetailsModal";
import PersonDetailModal from "@/apps/media/components/PersonDetailModal";
import { resolveStoragePath } from "@/lib/storage-url";
import { useLang, usePlayer } from "@/system";
import type {
  CreditOutput,
  GenreOutput,
  MediaExtraOutput,
  MediaFileOutput,
} from "@/types";

export function formatRuntime(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

export function formatFileSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(2)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${(bytes / 1e3).toFixed(0)} KB`;
}

export function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mb-3 text-base font-semibold text-gray-900 dark:text-gray-100">
      {children}
    </h3>
  );
}

export function MediaPoster({
  posterPath,
  title,
  fallbackEmoji,
}: {
  posterPath?: string | null;
  title: string;
  fallbackEmoji: string;
}) {
  return (
    <div className="hidden w-[160px] flex-shrink-0 overflow-hidden rounded-xl shadow-2xl md:block">
      {posterPath ? (
        <Image
          src={resolveStoragePath(posterPath)}
          alt={title}
          className="h-full w-full object-cover"
        />
      ) : (
        <div className="flex aspect-[2/3] items-center justify-center bg-[var(--bg-skeleton)] text-5xl">
          {fallbackEmoji}
        </div>
      )}
    </div>
  );
}

export function MediaTagsRow({
  genres,
  tmdbId,
  imdbId,
  tvdbId,
  mediaType,
}: {
  genres?: GenreOutput[] | null;
  tmdbId?: string | null;
  imdbId?: string | null;
  tvdbId?: string | null;
  mediaType?: "movie" | "tv";
}) {
  const { lang } = useLang();
  if (!genres?.length && !tmdbId && !imdbId && !tvdbId) return null;
  const tmdbUrl = tmdbId
    ? `https://www.themoviedb.org/${mediaType === "tv" ? "tv" : "movie"}/${tmdbId}`
    : null;
  const imdbUrl = imdbId ? `https://www.imdb.com/title/${imdbId}` : null;
  const tvdbUrl = tvdbId
    ? `https://www.thetvdb.com/?id=${tvdbId}&tab=series`
    : null;
  return (
    <>
      {genres?.map((g) => (
        <Tag key={g.id} color="default">
          {getGenreName(g.tmdbGenreId, lang) || g.name}
        </Tag>
      ))}
      {tmdbUrl && (
        <a href={tmdbUrl} target="_blank" rel="noopener noreferrer">
          <Tag color="green">TMDB</Tag>
        </a>
      )}
      {imdbUrl && (
        <a href={imdbUrl} target="_blank" rel="noopener noreferrer">
          <Tag color="orange">IMDB</Tag>
        </a>
      )}
      {tvdbUrl && (
        <a href={tvdbUrl} target="_blank" rel="noopener noreferrer">
          <Tag color="purple">TVDB</Tag>
        </a>
      )}
    </>
  );
}

export function MediaInfoBlock({
  directors,
  writers,
  date,
  dateLabel,
  countries,
}: {
  directors: string[];
  writers: string[];
  date?: string | null;
  dateLabel: string;
  countries?: string[] | null;
}) {
  const hasAny =
    directors.length > 0 ||
    writers.length > 0 ||
    !!date ||
    (countries?.length ?? 0) > 0;
  if (!hasAny) return null;
  return (
    <div className="mt-3 space-y-1 text-sm">
      {directors.length > 0 && (
        <div>
          <span className="font-semibold text-gray-900 dark:text-gray-100">
            导演:{" "}
          </span>
          <span className="text-gray-600 dark:text-zinc-400">
            {directors.join(", ")}
          </span>
        </div>
      )}
      {writers.length > 0 && (
        <div>
          <span className="font-semibold text-gray-900 dark:text-gray-100">
            编剧:{" "}
          </span>
          <span className="text-gray-600 dark:text-zinc-400">
            {writers.join(", ")}
          </span>
        </div>
      )}
      {date && (
        <div>
          <span className="font-semibold text-gray-900 dark:text-gray-100">
            {dateLabel}:{" "}
          </span>
          <span className="text-gray-600 dark:text-zinc-400">{date}</span>
        </div>
      )}
      {countries && countries.length > 0 && (
        <div>
          <span className="font-semibold text-gray-900 dark:text-gray-100">
            地区:{" "}
          </span>
          <span className="text-gray-600 dark:text-zinc-400">
            {countries.join(", ")}
          </span>
        </div>
      )}
    </div>
  );
}

export const PersonPlaceholder = () => (
  <div className="flex h-full items-center justify-center text-zinc-600 dark:text-gray-500">
    <svg className="h-10 w-10" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z" />
    </svg>
  </div>
);

export function PersonCard({
  personId,
  name,
  sub,
  profilePath,
}: {
  personId?: string;
  name: string;
  sub?: string | null;
  profilePath?: string | null;
}) {
  const [modalOpen, setModalOpen] = useState(false);
  const clickable = !!personId;
  return (
    <>
      <button
        type="button"
        className={`w-[110px] flex-shrink-0 overflow-hidden rounded-lg bg-gray-50 text-left dark:bg-gray-800/60 ${clickable ? "cursor-pointer hover:outline hover:outline-2 hover:outline-offset-1 hover:outline-primary/60" : "cursor-default"}`}
        onClick={() => personId && setModalOpen(true)}
        disabled={!clickable}
      >
        <div className="relative aspect-[2/3] overflow-hidden bg-[var(--bg-skeleton)]">
          {profilePath ? (
            <img
              src={resolveStoragePath(profilePath)}
              alt={name}
              className="h-full w-full object-cover"
              loading="lazy"
            />
          ) : (
            <PersonPlaceholder />
          )}
        </div>
        <div className="p-1.5">
          <p className="truncate text-xs font-medium text-gray-900 dark:text-gray-100">
            {name}
          </p>
          {sub && (
            <p className="truncate text-[11px] text-gray-500 dark:text-zinc-400">
              {sub}
            </p>
          )}
        </div>
      </button>
      {personId && modalOpen && (
        <PersonDetailModal
          personId={personId}
          character={sub}
          onClose={() => setModalOpen(false)}
        />
      )}
    </>
  );
}

export function CastRow({ credits }: { credits: CreditOutput[] }) {
  const actors = credits.filter((c) => c.role === "actor");
  if (!actors.length) return null;
  return (
    <section className="mb-8">
      <SectionTitle>演员</SectionTitle>
      <HorizontalScroll innerClassName="gap-3 px-0.5 pb-2 pt-0.5">
        {actors.map((c) => (
          <PersonCard
            key={c.id}
            personId={c.person.id}
            name={c.person.name}
            sub={c.character}
            profilePath={c.person.profilePath}
          />
        ))}
      </HorizontalScroll>
    </section>
  );
}

export function CrewRow({ credits }: { credits: CreditOutput[] }) {
  const crew = credits.filter((c) => c.role !== "actor");
  if (!crew.length) return null;
  return (
    <section className="mb-8">
      <SectionTitle>幕后</SectionTitle>
      <HorizontalScroll innerClassName="gap-3 px-0.5 pb-2 pt-0.5">
        {crew.map((c) => (
          <PersonCard
            key={c.id}
            personId={c.person.id}
            name={c.person.name}
            sub={
              c.role === "director"
                ? "导演"
                : c.role === "writer"
                  ? "编剧"
                  : c.role
            }
            profilePath={c.person.profilePath}
          />
        ))}
      </HorizontalScroll>
    </section>
  );
}

function formatResolution(height: number): string {
  if (height >= 2160) return "4K";
  if (height >= 1440) return "1440P";
  if (height >= 1080) return "1080P";
  if (height >= 720) return "720P";
  if (height >= 480) return "480P";
  return `${height}P`;
}

function formatVideoCodec(codec: string): string {
  const c = codec.toLowerCase();
  if (c === "h264" || c === "avc") return "H.264";
  if (c === "hevc" || c === "h265") return "HEVC";
  if (c === "av1") return "AV1";
  if (c === "vp9") return "VP9";
  if (c === "vp8") return "VP8";
  if (c === "mpeg4") return "MPEG-4";
  if (c === "mpeg2video" || c === "mpeg2") return "MPEG-2";
  return codec.toUpperCase();
}

/** Format HDR type for badge display — matches Jellyfin's 13 VideoRangeType values. */
function formatHdrBadge(hdrType: string): string {
  const HDR_BADGE_MAP: Record<string, string> = {
    hdr10: "HDR10",
    hdr10plus: "HDR10+",
    hdr10_plus: "HDR10+",
    hlg: "HLG",
    dolby_vision: "DV",
    dovi: "DV",
    dolby_vision_hdr10: "DV HDR10",
    dolby_vision_hdr10_plus: "DV HDR10+",
    dolby_vision_hlg: "DV HLG",
    dolby_vision_sdr: "DV SDR",
    dolby_vision_el: "DV EL",
    dolby_vision_el_hdr10_plus: "DV EL HDR10+",
    dovi_invalid: "DV Invalid",
  };
  return HDR_BADGE_MAP[hdrType.toLowerCase()] ?? hdrType.toUpperCase();
}

export function MediaFileCard({
  file,
  playMeta,
}: {
  file: MediaFileOutput;
  playMeta?: {
    title: string;
    posterPath?: string | null;
    movieId?: string;
    episodeId?: string;
    imdbId?: string | null;
    tmdbId?: string | null;
  };
}) {
  const { play } = usePlayer();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const fullPath = getMediaFileLocator(file);
  return (
    <>
      <div
        className={`group relative rounded-lg border p-3 backdrop-blur-sm transition-all ${
          detailsOpen
            ? "shadow-sm"
            : "border-[var(--glass-border)] bg-white/40 hover:border-black/10 hover:bg-white/70 hover:shadow-sm dark:bg-white/[0.03] dark:hover:border-white/[0.12] dark:hover:bg-white/[0.08]"
        }`}
        style={
          detailsOpen
            ? {
                borderColor: "var(--accent-subtle-hover)",
                background: "var(--accent-subtle)",
              }
            : undefined
        }
      >
        <button
          type="button"
          aria-label={`查看 ${file.filename} 的文件详情`}
          tabIndex={-1}
          className="absolute inset-0 z-0 cursor-pointer rounded-lg focus:outline-none"
          onClick={() => setDetailsOpen(true)}
        />
        <div className="relative z-10 mb-3 flex items-start justify-between gap-3 pointer-events-none">
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-baseline gap-1.5">
              {file.sourceName && (
                <span className="inline-flex flex-shrink-0 items-center rounded bg-emerald-100 px-1.5 py-0.5 text-[11px] font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400">
                  {file.sourceName}
                </span>
              )}
              <p
                className="min-w-0 truncate text-sm font-medium text-gray-900 dark:text-gray-100"
                title={file.filename}
              >
                {file.filename}
              </p>
            </div>
            <p
              className="mt-1 break-all font-mono text-[11px] text-gray-500 dark:text-zinc-400"
              title={fullPath}
            >
              {fullPath}
            </p>
          </div>
          <div className="pointer-events-auto relative z-10 flex flex-shrink-0 items-center gap-2">
            <button
              type="button"
              title="文件详情"
              aria-label="文件详情"
              className="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md border border-[var(--glass-border)] px-2.5 text-xs font-medium text-gray-600 transition-colors hover:bg-gray-50 hover:text-gray-900 dark:text-zinc-300 dark:hover:bg-gray-800/60 dark:hover:text-white"
              onClick={(event) => {
                event.stopPropagation();
                setDetailsOpen(true);
              }}
            >
              <Info className="h-3.5 w-3.5" />
              详情
            </button>
            {playMeta && (
              <button
                type="button"
                title="播放"
                className="inline-flex h-8 cursor-pointer flex-shrink-0 items-center gap-1.5 rounded-md bg-[var(--accent)] px-2.5 text-xs font-medium text-white hover:opacity-90"
                onClick={(event) => {
                  event.stopPropagation();
                  play(file, playMeta);
                }}
              >
                <Play className="h-3.5 w-3.5 fill-current" />
                播放
              </button>
            )}
          </div>
        </div>
        <div className="pointer-events-none relative z-10 flex flex-wrap gap-1.5">
          {file.size != null && (
            <Tag size="small" color="default">
              {formatFileSize(file.size)}
            </Tag>
          )}
          {file.videoCodec && (
            <Tag size="small" color="blue">
              {formatVideoCodec(file.videoCodec)}
              {file.videoProfile ? ` ${file.videoProfile}` : ""}
            </Tag>
          )}
          {file.videoHeight != null && (
            <Tag size="small" color="blue">
              {formatResolution(file.videoHeight)}
            </Tag>
          )}
          {file.hdrType && file.hdrType !== "sdr" && (
            <Tag size="small" color="gold">
              {formatHdrBadge(file.hdrType)}
            </Tag>
          )}
          {(
            file.audioStreams as
              | Array<{
                  codec_name?: string;
                  channels?: number;
                  tags?: { language?: string };
                }>
              | null
              | undefined
          )?.map((a, i) => (
            <Tag
              // biome-ignore lint/suspicious/noArrayIndexKey: audio streams lack unique IDs
              key={`audio-${i}-${a.tags?.language ?? ""}-${a.codec_name ?? ""}-${a.channels ?? ""}`}
              size="small"
              color="cyan"
            >
              {a.tags?.language && a.tags.language !== "und"
                ? `${a.tags.language.toUpperCase()} `
                : ""}
              {a.codec_name ? a.codec_name.toUpperCase() : "未知"}
              {a.channels
                ? ` ${a.channels === 2 ? "Stereo" : a.channels === 1 ? "Mono" : `${a.channels}ch`}`
                : ""}
            </Tag>
          ))}
          {file.subtitles?.map((sub) => (
            <Tag key={sub.id} size="small" color="purple">
              {sub.language.toUpperCase()}
              {sub.isDefault ? " ★" : ""}
              {sub.isForced ? " !" : ""}
            </Tag>
          ))}
        </div>
      </div>
      <FileDetailsModal
        file={file}
        open={detailsOpen}
        onClose={() => setDetailsOpen(false)}
        posterPath={playMeta?.posterPath}
      />
    </>
  );
}

export function FilesSection({
  files,
  playMeta,
}: {
  files: MediaFileOutput[];
  playMeta?: {
    title: string;
    posterPath?: string | null;
    movieId?: string;
    episodeId?: string;
    imdbId?: string | null;
    tmdbId?: string | null;
  };
}) {
  if (!files.length) return null;
  return (
    <section className="mb-8">
      <SectionTitle>文件</SectionTitle>
      <div className="space-y-2">
        {files.map((f) => (
          <MediaFileCard key={f.id} file={f} playMeta={playMeta} />
        ))}
      </div>
    </section>
  );
}

export function ExtrasSection({ extras }: { extras: MediaExtraOutput[] }) {
  if (!extras.length) return null;
  const typeLabel: Record<string, string> = {
    trailer: "预告片",
    behind_the_scenes: "幕后花絮",
    featurette: "花絮",
    interview: "访谈",
    scene: "片段",
    short: "短片",
    other: "其他",
  };
  return (
    <section className="mb-8">
      <SectionTitle>花絮 / 预告</SectionTitle>
      <div className="flex gap-3 overflow-x-auto pb-2">
        {extras.map((e) => (
          <div
            key={e.id}
            className="w-[200px] flex-shrink-0 overflow-hidden rounded-lg bg-gray-50 dark:bg-gray-800/60"
          >
            <div className="relative aspect-video overflow-hidden bg-[var(--bg-skeleton)]">
              {e.thumbPath ? (
                <Image
                  src={resolveStoragePath(e.thumbPath)}
                  alt={e.title}
                  className="h-full w-full object-cover"
                />
              ) : (
                <div className="flex h-full items-center justify-center text-3xl text-zinc-600 dark:text-zinc-400">
                  ▶
                </div>
              )}
              <span className="absolute bottom-1 left-1 rounded bg-black/60 px-1 py-0.5 text-[10px] text-white">
                {typeLabel[e.type] ?? e.type}
              </span>
            </div>
            <div className="p-1.5">
              <p className="truncate text-xs font-medium text-gray-900 dark:text-gray-100">
                {e.title}
              </p>
              {e.runtime != null && (
                <p className="text-[11px] text-zinc-600 dark:text-zinc-400">
                  {formatRuntime(e.runtime)}
                </p>
              )}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
