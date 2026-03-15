import { ArrowLeftOutlined, Button, Spin } from "@acme/components";
import type { EpisodeOutput } from "@acme/types";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useBackgroundArt } from "../../hooks";
import { trpc } from "../../lib/trpc";
import {
  CastRow,
  CrewRow,
  ExtrasSection,
  formatRuntime,
  MediaFileCard,
  MediaInfoBlock,
  MediaPoster,
  MediaTagsRow,
  SectionTitle,
} from "./media-detail-shared";

function FavoriteButton({
  isFavorite,
  tvShowId,
}: {
  isFavorite: boolean;
  tvShowId: string;
}) {
  const utils = trpc.useUtils();
  const toggle = trpc.mediaLibrary.toggleFavorite.useMutation({
    onSuccess: () =>
      void utils.mediaLibrary.getTvShowDetail.invalidate({ id: tvShowId }),
  });
  return (
    <button
      type="button"
      title={isFavorite ? "取消收藏" : "收藏"}
      className={`flex h-8 w-8 items-center justify-center rounded-full text-xl transition-transform hover:scale-110 ${isFavorite ? "text-red-500" : "text-gray-400 hover:text-red-400"}`}
      onClick={() => toggle.mutate({ type: "tvshow", id: tvShowId })}
    >
      {isFavorite ? "♥" : "♡"}
    </button>
  );
}

function EpisodeRow({ episode }: { episode: EpisodeOutput }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="overflow-hidden rounded-lg border border-[var(--glass-border)]">
      <button
        type="button"
        className="flex w-full items-start gap-4 p-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50"
        onClick={() => setOpen((v) => !v)}
      >
        {/* Thumbnail 16:9 */}
        <div className="h-[60px] w-[106px] flex-shrink-0 overflow-hidden rounded bg-gray-100 dark:bg-gray-800">
          {episode.stillPath ? (
            <img
              src={episode.stillPath}
              alt=""
              className="h-full w-full object-cover"
              loading="lazy"
            />
          ) : (
            <div className="flex h-full items-center justify-center text-xs font-medium text-gray-400">
              E{episode.episodeNumber}
            </div>
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-xs text-gray-500">
              第 {episode.episodeNumber} 集
            </span>
            {episode.runtime != null && (
              <span className="text-xs text-gray-400">
                {formatRuntime(episode.runtime)}
              </span>
            )}
            {episode.rating != null && (
              <span className="text-xs font-medium text-yellow-500">
                ★ {episode.rating.toFixed(1)}
              </span>
            )}
          </div>
          <p className="mt-0.5 text-sm font-semibold text-gray-900 dark:text-gray-100">
            {episode.title ?? `第 ${episode.episodeNumber} 集`}
          </p>
          {episode.airDate && (
            <p className="mt-0.5 text-xs text-gray-400">{episode.airDate}</p>
          )}
        </div>
        <span className="mt-1 flex-shrink-0 text-xs text-gray-400">
          {open ? "▲" : "▼"}
        </span>
      </button>

      {open && (
        <div className="border-t border-[var(--glass-border)] p-3">
          {episode.overview && (
            <p className="mb-3 text-sm text-gray-600 dark:text-gray-400">
              {episode.overview}
            </p>
          )}
          {episode.files && episode.files.length > 0 && (
            <div className="space-y-2">
              {episode.files.map((f) => (
                <MediaFileCard key={f.id} file={f} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function TvShowDetailPage() {
  const { id, tvId } = useParams<{ id: string; tvId: string }>();
  const navigate = useNavigate();
  const { setBackgroundArt } = useBackgroundArt();

  const { data: show, isLoading } = trpc.mediaLibrary.getTvShowDetail.useQuery(
    { id: tvId! },
    { enabled: !!tvId },
  );

  const seasons = show?.seasons ?? [];
  const [activeSeason, setActiveSeason] = useState<number | null>(null);
  const selectedSeason =
    seasons.find((s) => s.seasonNumber === activeSeason) ?? seasons[0];

  const [showStickyHeader, setShowStickyHeader] = useState(false);
  const [showSeasonInHeader, setShowSeasonInHeader] = useState(false);
  const scrollContainerRef = useRef<HTMLElement | null>(null);

  // React 19 callback ref — runs when h1 mounts (after data loads), returns cleanup
  const titleCallbackRef = useCallback(
    (node: HTMLHeadingElement | null): (() => void) | undefined => {
      if (!node) return;
      // Find the actual scroll container (overflow-y: auto) from parent chain
      let cursor: HTMLElement | null = node.parentElement;
      while (cursor && getComputedStyle(cursor).overflowY !== "auto") {
        cursor = cursor.parentElement;
      }
      scrollContainerRef.current = cursor;
      const observer = new IntersectionObserver(
        ([entry]) => setShowStickyHeader(!entry!.isIntersecting),
        { root: cursor ?? null, threshold: 0 },
      );
      observer.observe(node);
      return () => observer.disconnect();
    },
    [],
  );

  const seasonsCallbackRef = useCallback(
    (node: HTMLElement | null): (() => void) | undefined => {
      if (!node) return;
      // Same approach as titleCallbackRef: find the actual scroll container
      let cursor: HTMLElement | null = node.parentElement;
      while (cursor && getComputedStyle(cursor).overflowY !== "auto") {
        cursor = cursor.parentElement;
      }
      const observer = new IntersectionObserver(
        ([entry]) => setShowSeasonInHeader(!entry!.isIntersecting),
        { root: cursor ?? null, threshold: 0 },
      );
      observer.observe(node);
      return () => observer.disconnect();
    },
    [],
  );

  const handleSeasonClick = useCallback((seasonNumber: number) => {
    setActiveSeason(seasonNumber);
    scrollContainerRef.current?.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  useEffect(() => {
    if (show?.backdropPath) {
      setBackgroundArt(show.backdropPath);
    }
    return () => {
      setBackgroundArt(null);
    };
  }, [show?.backdropPath, setBackgroundArt]);

  if (isLoading) {
    return (
      <div className="flex h-96 items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!show) {
    return (
      <div className="flex h-96 flex-col items-center justify-center gap-4">
        <p className="text-gray-500">未找到该剂集</p>
        <Button onClick={() => navigate(`/dashboard/library/${id}`)}>
          返回
        </Button>
      </div>
    );
  }

  const directors = show.credits?.filter((c) => c.role === "director") ?? [];
  const writers = show.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = show.isFavorite ?? false;

  return (
    <div className="-mx-3 -mt-3 -mb-3 relative min-h-full lg:-mx-6 lg:-mt-6 lg:-mb-6">
      {/* ── Fixed scroll header (out of flow, no layout impact) ── */}
      <div
        className={`glass-sidebar fixed top-16 lg:top-0 left-0 right-0 lg:left-64 z-30 flex flex-col border-r-0 border-b border-b-[var(--border-base)] transition-all duration-200 ${
          showStickyHeader
            ? "opacity-100 translate-y-0"
            : "opacity-0 -translate-y-full pointer-events-none"
        }`}
      >
        {/* Row 1: back + title info */}
        <div className="flex items-center gap-3 px-4 py-3">
          <button
            type="button"
            className="flex cursor-pointer items-center gap-1.5 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
            onClick={() => navigate(`/dashboard/library/${id}`)}
          >
            <ArrowLeftOutlined />
            <span>返回</span>
          </button>
          <div className="mx-1 h-5 w-px bg-gray-300 dark:bg-gray-600" />
          {show.posterPath && (
            <img
              src={show.posterPath}
              alt={show.title}
              className="h-8 w-[21px] flex-shrink-0 rounded object-cover shadow"
            />
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate font-semibold text-sm text-gray-900 dark:text-gray-100">
              {show.title}
            </p>
            <div className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
              {show.originalTitle && show.originalTitle !== show.title && (
                <span className="truncate max-w-[120px]">
                  {show.originalTitle}
                </span>
              )}
              {show.year && <span>{show.year}</span>}
              {show.genres && show.genres.length > 0 && (
                <div className="flex gap-1">
                  {show.genres.slice(0, 3).map((g) => (
                    <span
                      key={g.id}
                      className="rounded-full bg-gray-100 dark:bg-white/10 px-2 py-px text-[11px] text-gray-700 dark:text-gray-300"
                    >
                      {g.name}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
        {/* Row 2: season switcher (only when seasons section scrolled out) */}
        {showSeasonInHeader && seasons.length > 0 && (
          <div className="flex justify-center gap-2 overflow-x-auto border-t border-[var(--border-base)] px-4 py-2">
            {seasons.map((sn) => (
              <button
                key={sn.id}
                type="button"
                onClick={() => handleSeasonClick(sn.seasonNumber)}
                className={`flex flex-shrink-0 cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 transition-colors ${
                  selectedSeason?.id === sn.id
                    ? "bg-gray-100 dark:bg-white/10 text-gray-900 dark:text-white"
                    : "hover:bg-gray-100 dark:hover:bg-white/8 text-gray-700 dark:text-gray-300"
                }`}
              >
                {sn.posterPath ? (
                  <img
                    src={sn.posterPath}
                    alt=""
                    className="h-8 w-[22px] flex-shrink-0 rounded object-cover"
                    loading="lazy"
                  />
                ) : (
                  <div className="flex h-8 w-[22px] flex-shrink-0 items-center justify-center rounded bg-white/20 text-[10px]">
                    S{sn.seasonNumber}
                  </div>
                )}
                <div className="text-left">
                  <p className="text-xs font-semibold leading-tight">
                    {sn.title ?? `第 ${sn.seasonNumber} 季`}
                  </p>
                  {sn.episodeCount != null && (
                    <p
                      className={`text-[11px] leading-tight ${
                        selectedSeason?.id === sn.id
                          ? "text-white/70"
                          : "text-gray-400"
                      }`}
                    >
                      {sn.episodeCount} 集
                    </p>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* ── Header ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        <div className="mb-6">
          <Button
            icon={<ArrowLeftOutlined />}
            onClick={() => navigate(`/dashboard/library/${id}`)}
          >
            返回
          </Button>
        </div>
        <div className="flex items-start gap-6">
          <MediaPoster
            posterPath={show.posterPath}
            title={show.title}
            fallbackEmoji="📺"
          />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h1
                ref={titleCallbackRef}
                className="text-3xl font-bold leading-tight"
              >
                {show.title}
              </h1>
              <FavoriteButton isFavorite={isFavorite} tvShowId={show.id} />
            </div>
            {show.originalTitle && show.originalTitle !== show.title && (
              <p className="mt-0.5 text-sm text-gray-500 dark:text-gray-400">
                {show.originalTitle}
              </p>
            )}
            <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
              {show.year && (
                <span className="text-gray-600 dark:text-gray-300">
                  {show.year}
                </span>
              )}
              {seasons.length > 0 && (
                <span className="text-gray-600 dark:text-gray-300">
                  · {seasons.length} 季
                </span>
              )}
              {show.contentRating && (
                <span className="rounded border border-[var(--glass-border)] px-1.5 py-0.5 text-xs text-gray-600 dark:text-gray-300">
                  {show.contentRating}
                </span>
              )}
              {show.status && (
                <span
                  className={`rounded px-1.5 py-0.5 text-xs font-medium ${
                    show.status === "ended"
                      ? "bg-gray-200/60 dark:bg-gray-600/60 text-gray-700 dark:text-gray-300"
                      : "bg-green-100/60 dark:bg-green-600/60 text-green-700 dark:text-green-200"
                  }`}
                >
                  {show.status}
                </span>
              )}
              {show.rating != null && (
                <span className="rounded bg-yellow-500/20 px-2 py-0.5 text-xs font-semibold text-yellow-600 dark:text-yellow-400">
                  ★ {show.rating.toFixed(1)}
                </span>
              )}
              <MediaTagsRow
                genres={show.genres}
                tmdbId={show.tmdbId}
                imdbId={show.imdbId}
                tvdbId={show.tvdbId}
                mediaType="tv"
              />
            </div>
            <MediaInfoBlock
              directors={directors.map((d) => d.person.name)}
              writers={writers.map((w) => w.person.name)}
              date={show.firstAirDate}
              dateLabel="首播"
              countries={show.countries}
            />
          </div>
        </div>
      </div>

      {/* ── Body ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        {show.overview && (
          <div className="mb-6">
            <SectionTitle>简介</SectionTitle>
            <p className="text-sm leading-relaxed text-gray-700 dark:text-gray-300">
              {show.overview}
            </p>
          </div>
        )}

        {/* Season + Episodes */}
        {seasons.length > 0 && (
          <section className="mb-8">
            {/* Season selector — horizontal scrollable row */}
            <div
              ref={seasonsCallbackRef}
              className="flex flex-row justify-center gap-2 overflow-x-auto pb-3"
            >
              {seasons.map((sn) => (
                <button
                  key={sn.id}
                  type="button"
                  onClick={() => setActiveSeason(sn.seasonNumber)}
                  className={`flex cursor-pointer flex-shrink-0 items-center gap-2.5 rounded-lg p-2 text-left transition-colors ${
                    selectedSeason?.id === sn.id
                      ? "dark:bg-white/10"
                      : "hover:bg-gray-100 dark:hover:bg-white/5"
                  }`}
                  style={
                    selectedSeason?.id === sn.id
                      ? {
                          background: "var(--accent-subtle)",
                          color: "var(--accent)",
                        }
                      : undefined
                  }
                >
                  {sn.posterPath ? (
                    <img
                      src={sn.posterPath}
                      alt=""
                      className="h-10 w-7 flex-shrink-0 rounded object-cover"
                      loading="lazy"
                    />
                  ) : (
                    <div className="flex h-10 w-7 flex-shrink-0 items-center justify-center rounded bg-gray-200 text-xs dark:bg-gray-700">
                      S
                    </div>
                  )}
                  <div className="min-w-0">
                    <p className="truncate text-xs font-semibold">
                      {sn.title ?? `第 ${sn.seasonNumber} 季`}
                    </p>
                    {sn.episodeCount != null && (
                      <p className="text-[11px] text-gray-400">
                        {sn.episodeCount} 集
                      </p>
                    )}
                  </div>
                </button>
              ))}
            </div>

            {/* Episode list */}
            {selectedSeason && (
              <div className="space-y-2">
                {(selectedSeason.episodes ?? []).map((ep) => (
                  <EpisodeRow key={ep.id} episode={ep} />
                ))}
                {(selectedSeason.episodes ?? []).length === 0 && (
                  <p className="py-8 text-center text-sm text-gray-400">
                    暂无剧集数据
                  </p>
                )}
              </div>
            )}
          </section>
        )}

        {/* Collections */}
        {show.collections && show.collections.length > 0 && (
          <section className="mb-8">
            <SectionTitle>所属合集</SectionTitle>
            <div className="flex gap-3 overflow-x-auto pb-2">
              {show.collections.map((col) => (
                <div
                  key={col.id}
                  className="flex w-[200px] flex-shrink-0 items-center gap-2.5 rounded-lg border border-[var(--glass-border)] p-2"
                >
                  {col.posterPath && (
                    <img
                      src={col.posterPath}
                      alt={col.name}
                      className="h-12 w-8 flex-shrink-0 rounded object-cover"
                    />
                  )}
                  <p className="truncate text-xs font-medium text-gray-900 dark:text-gray-100">
                    {col.name}
                  </p>
                </div>
              ))}
            </div>
          </section>
        )}

        {/* Cast */}
        <CastRow credits={show.credits ?? []} />

        {/* Crew */}
        <CrewRow credits={show.credits ?? []} />

        <ExtrasSection extras={show.extras ?? []} />
      </div>
    </div>
  );
}
