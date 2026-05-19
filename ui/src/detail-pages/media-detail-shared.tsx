import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  shift,
  size,
  useFloating,
} from "@floating-ui/react";
import { posterThumbUrl } from "@tokimo/sdk";
import { cn, Popover, ScrollArea, Tag } from "@tokimo/ui";
import { Play } from "lucide-react";
import { useCallback, useRef, useState } from "react";
import {
  type CreditOutput,
  type GenreOutput,
  getGenreName,
  type MediaFileOutput,
} from "../api";
import { useLang, usePlayer } from "../hooks/shell-stubs";
import {
  FileDetailsTooltipContent,
  getMediaFileLocator,
} from "../shell-shim/apps-finder";
import { PersonDetailPopoverContent } from "../shell-shim/apps-media";

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
    <h3 className="mb-3 text-base font-semibold text-fg-primary">{children}</h3>
  );
}

export function MediaPoster({
  posterPath,
  title,
  fallbackEmoji,
  landscape,
}: {
  posterPath?: string | null;
  title: string;
  fallbackEmoji: string;
  landscape?: boolean;
}) {
  return (
    <div
      className={cn(
        "hidden flex-shrink-0 overflow-hidden rounded-xl shadow-2xl md:block",
        landscape ? "aspect-video w-[320px]" : "aspect-[2/3] w-[160px]",
      )}
    >
      {posterPath ? (
        <img
          src={posterThumbUrl(posterPath, 300)}
          alt={title}
          className="h-full w-full object-cover"
        />
      ) : (
        <div className="flex h-full w-full items-center justify-center bg-[var(--bg-skeleton)] text-5xl">
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
          <span className="font-semibold text-fg-primary">导演: </span>
          <span className="text-fg-muted">{directors.join(", ")}</span>
        </div>
      )}
      {writers.length > 0 && (
        <div>
          <span className="font-semibold text-fg-primary">编剧: </span>
          <span className="text-fg-muted">{writers.join(", ")}</span>
        </div>
      )}
      {date && (
        <div>
          <span className="font-semibold text-fg-primary">{dateLabel}: </span>
          <span className="text-fg-muted">{date}</span>
        </div>
      )}
      {countries && countries.length > 0 && (
        <div>
          <span className="font-semibold text-fg-primary">地区: </span>
          <span className="text-fg-muted">{countries.join(", ")}</span>
        </div>
      )}
    </div>
  );
}

export const PersonPlaceholder = () => (
  <div className="flex h-full items-center justify-center text-fg-muted">
    <svg className="h-10 w-10" viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z" />
    </svg>
  </div>
);

interface HoveredPerson {
  personId: string;
  character?: string | null;
}

function usePersonPanel() {
  const [hovered, setHovered] = useState<HoveredPerson | null>(null);
  // mounted: keeps DOM alive during fade-out; visible: controls opacity
  const [mounted, setMounted] = useState(false);
  const [visible, setVisible] = useState(false);
  // Suppress transform transition on first position to avoid fly-from-corner
  const [sliding, setSliding] = useState(false);
  const leaveTimer = useRef<number>(0);
  const fadeOutTimer = useRef<number>(0);

  const { refs, floatingStyles } = useFloating({
    open: mounted,
    placement: "bottom-start",
    middleware: [
      offset(8),
      flip(),
      shift({ padding: 8 }),
      size({
        padding: 16,
        apply({ availableHeight, elements }) {
          Object.assign(elements.floating.style, {
            maxHeight: `${availableHeight}px`,
            overflowY: "auto",
          });
        },
      }),
    ],
    whileElementsMounted: autoUpdate,
  });

  const cancelLeave = useCallback(() => {
    clearTimeout(leaveTimer.current);
    clearTimeout(fadeOutTimer.current);
  }, []);

  const enter = useCallback(
    (el: HTMLElement, data: HoveredPerson) => {
      clearTimeout(leaveTimer.current);
      clearTimeout(fadeOutTimer.current);
      const wasVisible = visible;
      refs.setReference(el);
      setHovered(data);
      setMounted(true);
      // Enable slide transition only when switching between cards
      if (wasVisible) {
        setSliding(true);
      } else {
        setSliding(false);
      }
      requestAnimationFrame(() => setVisible(true));
    },
    [refs, visible],
  );

  const leave = useCallback(() => {
    leaveTimer.current = window.setTimeout(() => {
      setVisible(false);
      setSliding(false);
      fadeOutTimer.current = window.setTimeout(() => {
        setMounted(false);
        setHovered(null);
      }, 150);
    }, 100);
  }, []);

  return {
    hovered,
    mounted,
    visible,
    sliding,
    enter,
    leave,
    cancelLeave,
    refs,
    floatingStyles,
  };
}

function PersonPanel({
  hovered,
  visible,
  sliding,
  refs,
  floatingStyles,
  cancelLeave,
  leave,
}: {
  hovered: HoveredPerson;
  visible: boolean;
  sliding: boolean;
  refs: ReturnType<typeof useFloating>["refs"];
  floatingStyles: React.CSSProperties;
  cancelLeave: () => void;
  leave: () => void;
}) {
  return (
    <FloatingPortal>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: panel hover keeps it open */}
      <div
        ref={refs.setFloating}
        style={{
          ...floatingStyles,
          opacity: visible ? 1 : 0,
          transition: [
            "opacity 150ms ease",
            sliding ? "transform 200ms ease" : undefined,
          ]
            .filter(Boolean)
            .join(", "),
          backdropFilter: "blur(var(--window-blur, 24px))",
          WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
          borderRadius: "var(--window-radius, 10px)",
        }}
        className="z-[9999] w-[400px] overflow-hidden border border-black/[0.06] p-3 shadow-xl bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] dark:border-white/[0.08] dark:bg-[rgba(15,15,25,calc(var(--window-opacity,85)/100))]"
        onMouseEnter={cancelLeave}
        onMouseLeave={leave}
      >
        <PersonDetailPopoverContent
          personId={hovered.personId}
          character={hovered.character}
        />
      </div>
    </FloatingPortal>
  );
}

export function PersonCard({
  name,
  sub,
  profilePath,
  onMouseEnter,
}: {
  name: string;
  sub?: string | null;
  profilePath?: string | null;
  onMouseEnter?: (el: HTMLElement) => void;
}) {
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: hover triggers shared panel
    <div
      className="w-[110px] flex-shrink-0 cursor-pointer overflow-hidden rounded-lg bg-surface-elevated text-left hover:outline hover:outline-2 hover:outline-offset-1 hover:outline-primary/60"
      onMouseEnter={(e) => onMouseEnter?.(e.currentTarget)}
    >
      <div className="relative aspect-[2/3] overflow-hidden bg-[var(--bg-skeleton)]">
        {profilePath ? (
          <img
            src={posterThumbUrl(profilePath, 340)}
            alt={name}
            className="h-full w-full object-cover"
            loading="lazy"
          />
        ) : (
          <PersonPlaceholder />
        )}
      </div>
      <div className="p-1.5">
        <p className="truncate text-xs font-medium text-fg-primary">{name}</p>
        {sub && <p className="truncate text-[11px] text-fg-muted">{sub}</p>}
      </div>
    </div>
  );
}

export function CastRow({ credits }: { credits: CreditOutput[] }) {
  const actors = credits.filter((c) => c.role === "actor");
  const {
    hovered,
    mounted,
    visible,
    sliding,
    enter,
    leave,
    cancelLeave,
    refs,
    floatingStyles,
  } = usePersonPanel();
  if (!actors.length) return null;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: section-level mouse leave closes panel
    <section className="mb-8" onMouseLeave={leave}>
      <SectionTitle>演员</SectionTitle>
      <ScrollArea
        direction="horizontal"
        hideScrollbar
        innerClassName="flex gap-3 px-0.5 pb-2 pt-0.5"
      >
        {actors.map((c) => (
          <PersonCard
            key={c.id}
            name={c.person.name}
            sub={c.character}
            profilePath={c.person.profilePath}
            onMouseEnter={(el) =>
              enter(el, {
                personId: c.person.id,
                character: c.character,
              })
            }
          />
        ))}
      </ScrollArea>
      {mounted && hovered && (
        <PersonPanel
          hovered={hovered}
          visible={visible}
          sliding={sliding}
          refs={refs}
          floatingStyles={floatingStyles}
          cancelLeave={cancelLeave}
          leave={leave}
        />
      )}
    </section>
  );
}

export function CrewRow({ credits }: { credits: CreditOutput[] }) {
  const crew = credits.filter((c) => c.role !== "actor");
  const {
    hovered,
    mounted,
    visible,
    sliding,
    enter,
    leave,
    cancelLeave,
    refs,
    floatingStyles,
  } = usePersonPanel();
  if (!crew.length) return null;
  const roleName = (role: string) =>
    role === "director" ? "导演" : role === "writer" ? "编剧" : role;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: section-level mouse leave closes panel
    <section className="mb-8" onMouseLeave={leave}>
      <SectionTitle>幕后</SectionTitle>
      <ScrollArea
        direction="horizontal"
        hideScrollbar
        innerClassName="flex gap-3 px-0.5 pb-2 pt-0.5"
      >
        {crew.map((c) => (
          <PersonCard
            key={c.id}
            name={c.person.name}
            sub={roleName(c.role)}
            profilePath={c.person.profilePath}
            onMouseEnter={(el) =>
              enter(el, {
                personId: c.person.id,
                character: c.role === "actor" ? c.character : roleName(c.role),
              })
            }
          />
        ))}
      </ScrollArea>
      {mounted && hovered && (
        <PersonPanel
          hovered={hovered}
          visible={visible}
          sliding={sliding}
          refs={refs}
          floatingStyles={floatingStyles}
          cancelLeave={cancelLeave}
          leave={leave}
        />
      )}
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
    videoItemId?: string;
    episodeId?: string;
    imdbId?: string | null;
    tmdbId?: string | null;
  };
}) {
  const { play } = usePlayer();
  const fullPath = getMediaFileLocator(file);
  return (
    <Popover
      trigger="click"
      placement="bottomLeft"
      fitViewport
      popupClassName="border border-black/[0.06] dark:border-white/[0.08] shadow-xl w-[720px] p-3 bg-white/90 dark:bg-[rgba(15,15,25,0.9)]"
      content={<FileDetailsTooltipContent file={file} />}
    >
      <div className="group relative cursor-pointer rounded-lg border border-border-base bg-white/40 p-3 backdrop-blur-sm transition-all hover:border-black/10 hover:bg-white/70 hover:shadow-sm dark:bg-white/[0.03] dark:hover:border-white/[0.12] dark:hover:bg-white/[0.08]">
        <div className="mb-3 flex items-start justify-between gap-3 pointer-events-none">
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-baseline gap-1.5">
              {file.sourceName && (
                <span className="inline-flex flex-shrink-0 items-center rounded bg-emerald-100 px-1.5 py-0.5 text-[11px] font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400">
                  {file.sourceName}
                </span>
              )}
              <p
                className="min-w-0 truncate text-sm font-medium text-fg-primary"
                title={file.filename}
              >
                {file.filename}
              </p>
            </div>
            <p
              className="mt-1 break-all font-mono text-[11px] text-fg-muted"
              title={fullPath}
            >
              {fullPath}
            </p>
          </div>
          {playMeta && (
            <div className="pointer-events-auto relative z-10 flex flex-shrink-0 items-center gap-2">
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
            </div>
          )}
        </div>
        <div className="pointer-events-none flex flex-wrap gap-1.5">
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
    </Popover>
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
    videoItemId?: string;
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
