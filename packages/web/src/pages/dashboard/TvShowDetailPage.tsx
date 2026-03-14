import { ArrowLeftOutlined, Button, Image, Spin, Tag } from "@acme/components";
import type { EpisodeOutput } from "@acme/types";
import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useBackgroundArt } from "../../hooks";
import { trpc } from "../../lib/trpc";
import {
  CastRow,
  CrewRow,
  ExtrasSection,
  formatRuntime,
  SectionTitle,
} from "./media-detail-shared";

function formatFileSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(2)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${(bytes / 1e3).toFixed(0)} KB`;
}

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
    <div className="overflow-hidden rounded-lg border border-gray-100 dark:border-gray-700">
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
        <div className="border-t border-gray-100 p-3 dark:border-gray-700">
          {episode.overview && (
            <p className="mb-3 text-sm text-gray-600 dark:text-gray-400">
              {episode.overview}
            </p>
          )}
          {episode.files && episode.files.length > 0 && (
            <div className="space-y-1.5">
              {episode.files.map((f) => (
                <div key={f.id} className="text-xs text-gray-500">
                  <p className="font-medium text-gray-700 dark:text-gray-300">
                    {f.filename}
                  </p>
                  <div className="mt-0.5 flex flex-wrap gap-x-4 gap-y-0">
                    {f.size != null && (
                      <span>大小: {formatFileSize(f.size)}</span>
                    )}
                    {f.videoCodec && (
                      <span>
                        {f.videoCodec.toUpperCase()}
                        {f.videoWidth && f.videoHeight
                          ? ` ${f.videoWidth}×${f.videoHeight}`
                          : ""}
                      </span>
                    )}
                    {f.audioCodec && (
                      <span>
                        {f.audioCodec.toUpperCase()}
                        {f.audioChannels ? ` ${f.audioChannels}ch` : ""}
                      </span>
                    )}
                  </div>
                </div>
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

  // React 19 callback ref — runs when h1 mounts (after data loads), returns cleanup
  const titleCallbackRef = useCallback(
    (node: HTMLHeadingElement | null): (() => void) | undefined => {
      if (!node) return;
      const observer = new IntersectionObserver(
        ([entry]) => setShowStickyHeader(!entry!.isIntersecting),
        { threshold: 0 },
      );
      observer.observe(node);
      return () => observer.disconnect();
    },
    [],
  );

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
      {/* ── Sticky scroll header ── */}
      <div
        className={`sticky top-0 z-30 flex items-center gap-3 px-4 py-2 backdrop-blur-md bg-white/80 dark:bg-gray-900/80 border-b border-gray-200/60 dark:border-gray-700/60 shadow-sm transition-all duration-200 ${
          showStickyHeader
            ? "opacity-100 translate-y-0"
            : "opacity-0 -translate-y-full pointer-events-none"
        }`}
      >
        <button
          type="button"
          className="flex items-center gap-1.5 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
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
          <div className="hidden w-[160px] flex-shrink-0 overflow-hidden rounded-xl shadow-2xl md:block">
            {show.posterPath ? (
              <Image
                src={show.posterPath}
                alt={show.title}
                className="h-full w-full object-cover"
              />
            ) : (
              <div className="flex aspect-[2/3] items-center justify-center bg-gray-700 text-5xl">
                📺
              </div>
            )}
          </div>
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
                <span className="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 dark:border-gray-600 dark:text-gray-300">
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
            </div>
            {show.genres && show.genres.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1.5">
                {show.genres.map((g) => (
                  <span
                    key={g.id}
                    className="rounded-full bg-gray-100 px-2.5 py-0.5 text-xs text-gray-700 dark:bg-white/10 dark:text-gray-200"
                  >
                    {g.name}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* ── Body ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        {/* Overview + info sidebar */}
        <div className="mb-6 flex flex-col gap-6 md:flex-row">
          {show.overview && (
            <div className="flex-1">
              <SectionTitle>简介</SectionTitle>
              <p className="text-sm leading-relaxed text-gray-700 dark:text-gray-300">
                {show.overview}
              </p>
            </div>
          )}
          <div className="w-full shrink-0 space-y-2 text-sm md:w-52">
            {directors.length > 0 && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  导演:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {directors.map((d) => d.person.name).join(", ")}
                </span>
              </div>
            )}
            {writers.length > 0 && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  编剧:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {writers.map((w) => w.person.name).join(", ")}
                </span>
              </div>
            )}
            {show.firstAirDate && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  首播:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {show.firstAirDate}
                </span>
              </div>
            )}
            {show.countries && show.countries.length > 0 && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  地区:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {show.countries.join(", ")}
                </span>
              </div>
            )}
            {(show.tmdbId || show.imdbId || show.tvdbId) && (
              <div className="flex flex-wrap gap-1.5 pt-1">
                {show.tmdbId && <Tag color="green">TMDB</Tag>}
                {show.imdbId && <Tag color="orange">IMDB</Tag>}
                {show.tvdbId && <Tag color="purple">TVDB</Tag>}
              </div>
            )}
          </div>
        </div>

        {/* Season + Episodes */}
        {seasons.length > 0 && (
          <section className="mb-8">
            <SectionTitle>季度</SectionTitle>
            <div className="flex gap-5 md:flex-row flex-col">
              {/* Season selector */}
              <div className="flex flex-row gap-2 overflow-x-auto pb-1 md:flex-col md:w-44 md:shrink-0 md:overflow-x-visible md:pb-0">
                {seasons.map((sn) => (
                  <button
                    key={sn.id}
                    type="button"
                    onClick={() => setActiveSeason(sn.seasonNumber)}
                    className={`flex w-[150px] flex-shrink-0 items-center gap-2.5 rounded-lg p-2 text-left transition-colors md:w-full ${
                      selectedSeason?.id === sn.id
                        ? "bg-primary/10 text-primary"
                        : "hover:bg-gray-100 dark:hover:bg-gray-800"
                    }`}
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
                <div className="flex-1 space-y-2">
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
            </div>
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
                  className="flex w-[200px] flex-shrink-0 items-center gap-2.5 rounded-lg border border-gray-100 p-2 dark:border-gray-700"
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
