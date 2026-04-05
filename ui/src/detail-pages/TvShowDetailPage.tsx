import { useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeftOutlined,
  Button,
  PillTabBar,
  Spin,
} from "@tokiomo/components";
import { Play } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "@/generated/rust-api";
import { posterThumbUrl } from "@/lib/thumb";
import { useBackgroundArt, usePlayer, useWindowNav } from "@/system";
import type { EpisodeOutput } from "@/types";
import {
  CastRow,
  CrewRow,
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
  const qc = useQueryClient();
  const toggle = api.app.toggleFavorite.useMutation({
    onSuccess: () =>
      void api.app.getTvShowDetail.invalidate(qc, { id: tvShowId }),
  });
  return (
    <button
      type="button"
      title={isFavorite ? "取消收藏" : "收藏"}
      className={`flex h-8 w-8 items-center justify-center rounded-full text-xl transition-transform hover:scale-110 ${isFavorite ? "text-red-500" : "text-fg-muted hover:text-red-400"}`}
      onClick={() => toggle.mutate({ type: "tvshow", id: tvShowId })}
    >
      {isFavorite ? "♥" : "♡"}
    </button>
  );
}

function EpisodeRow({
  episode,
  playMeta,
}: {
  episode: EpisodeOutput;
  playMeta: {
    title: string;
    posterPath?: string | null;
    episodeId?: string;
    imdbId?: string | null;
    tmdbId?: string | null;
  };
}) {
  const [open, setOpen] = useState(false);
  const { play } = usePlayer();
  const firstFile = episode.files?.[0];
  return (
    <div className="overflow-hidden rounded-lg border border-[var(--glass-border)]">
      {/* Entire row is the expand click target */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: desktop-only UI, can't nest <button> */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: desktop-only UI */}
      <div
        className="flex w-full cursor-pointer items-stretch gap-4 p-3 transition-colors hover:bg-fill-tertiary/50"
        onClick={() => setOpen((v) => !v)}
      >
        {/* Thumbnail — click to play, stopPropagation prevents expand */}
        <button
          type="button"
          disabled={!firstFile}
          className="group/thumb relative h-[60px] w-[106px] flex-shrink-0 self-start cursor-pointer overflow-hidden rounded bg-fill-tertiary"
          onClick={(e) => {
            e.stopPropagation();
            if (firstFile)
              play(firstFile, {
                ...playMeta,
                title: episode.title ?? `第 ${episode.episodeNumber} 集`,
                episodeId: episode.id,
              });
          }}
        >
          {episode.stillPath ? (
            <img
              src={posterThumbUrl(episode.stillPath, 400)}
              alt=""
              className="h-full w-full object-cover"
              loading="lazy"
            />
          ) : (
            <div className="flex h-full items-center justify-center text-xs font-medium text-fg-muted">
              E{episode.episodeNumber}
            </div>
          )}
          {firstFile && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 transition-opacity group-hover/thumb:opacity-100">
              <div className="flex h-8 w-8 items-center justify-center rounded-full bg-white/90 shadow-md">
                <Play className="h-4 w-4 translate-x-0.5 fill-black text-black" />
              </div>
            </div>
          )}
        </button>

        {/* Info — visual only, parent div handles expand click */}
        <div className="flex min-w-0 flex-1 items-start gap-2">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span className="text-xs text-fg-muted">
                第 {episode.episodeNumber} 集
              </span>
              {episode.runtime != null && (
                <span className="text-xs text-fg-muted">
                  {formatRuntime(episode.runtime)}
                </span>
              )}
              {episode.rating != null && (
                <span className="text-xs font-medium text-yellow-500">
                  ★ {episode.rating.toFixed(1)}
                </span>
              )}
            </div>
            <p className="mt-0.5 text-sm font-semibold text-fg-primary">
              {episode.title ?? `第 ${episode.episodeNumber} 集`}
            </p>
            {episode.airDate && (
              <p className="mt-0.5 text-xs text-fg-muted">{episode.airDate}</p>
            )}
          </div>
          <span className="flex-shrink-0 self-center text-xs text-fg-muted">
            {open ? "▲" : "▼"}
          </span>
        </div>
      </div>

      {open && (
        <div className="border-t border-[var(--glass-border)] p-3">
          {episode.overview && (
            <p className="mb-3 text-sm text-fg-muted">{episode.overview}</p>
          )}
          {episode.files && episode.files.length > 0 && (
            <div className="space-y-2">
              {episode.files.map((f) => (
                <MediaFileCard
                  key={f.id}
                  file={f}
                  playMeta={{
                    ...playMeta,
                    title: episode.title ?? `第 ${episode.episodeNumber} 集`,
                    episodeId: episode.id,
                  }}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function TvShowDetailPage() {
  const { params, goBack } = useWindowNav();
  const tvId = params.tvShowId;
  const { setBackgroundArt } = useBackgroundArt();

  const { data: show, isLoading } = api.app.getTvShowDetail.useQuery(
    { id: tvId! },
    { enabled: !!tvId },
  );

  const seasons = show?.seasons ?? [];
  const [activeSeason, setActiveSeason] = useState<number | null>(null);
  const selectedSeason =
    seasons.find((s) => s.seasonNumber === activeSeason) ?? seasons[0];

  useEffect(() => {
    if (show?.backdropPath) {
      setBackgroundArt(posterThumbUrl(show.backdropPath, 1280) ?? null);
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
        <p className="text-fg-muted">未找到该剂集</p>
        <Button onClick={() => goBack()}>返回</Button>
      </div>
    );
  }

  const directors = show.credits?.filter((c) => c.role === "director") ?? [];
  const writers = show.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = show.isFavorite ?? false;

  return (
    <div className="-mx-3 -mt-3 -mb-3 relative min-h-full lg:-mx-4 lg:-mt-4 lg:-mb-4">
      {/* ── Header ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        <div className="mb-6">
          <Button icon={<ArrowLeftOutlined />} onClick={() => goBack()}>
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
              <h1 className="text-3xl font-bold leading-tight">{show.title}</h1>
              <FavoriteButton isFavorite={isFavorite} tvShowId={show.id} />
            </div>
            {show.originalTitle && show.originalTitle !== show.title && (
              <p className="mt-0.5 text-sm text-fg-muted">
                {show.originalTitle}
              </p>
            )}
            <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
              {show.year && (
                <span className="text-fg-secondary">{show.year}</span>
              )}
              {seasons.length > 0 && (
                <span className="text-fg-secondary">· {seasons.length} 季</span>
              )}
              {show.contentRating && (
                <span className="rounded border border-[var(--glass-border)] px-1.5 py-0.5 text-xs text-fg-secondary">
                  {show.contentRating}
                </span>
              )}
              {show.status && (
                <span
                  className={`rounded px-1.5 py-0.5 text-xs font-medium ${
                    show.status === "ended"
                      ? "bg-fill-tertiary text-fg-secondary"
                      : "bg-green-100/60 dark:bg-green-600/60 text-green-700 dark:text-green-200"
                  }`}
                >
                  {show.status}
                </span>
              )}
              {show.tmdbRating != null && (
                <span className="rounded bg-yellow-500/20 px-2 py-0.5 text-xs font-semibold text-yellow-600 dark:text-yellow-400">
                  TMDB ★ {show.tmdbRating.toFixed(1)}
                </span>
              )}
              {show.imdbRating != null && (
                <span className="rounded bg-amber-500/20 px-2 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400">
                  IMDb ★ {show.imdbRating.toFixed(1)}
                </span>
              )}
              {show.doubanRating != null && (
                <span className="rounded bg-green-500/20 px-2 py-0.5 text-xs font-semibold text-green-600 dark:text-green-400">
                  豆瓣 ★ {show.doubanRating.toFixed(1)}
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
            <p className="text-sm leading-relaxed text-fg-secondary">
              {show.overview}
            </p>
          </div>
        )}

        {/* Season + Episodes */}
        {seasons.length > 0 && (
          <section className="mb-8">
            {/* Season selector — PillTabBar */}
            <PillTabBar
              tabs={seasons.map((sn) => {
                const base = `第 ${sn.seasonNumber} 季`;
                // Only append title if it's a real scraped name (not generic "Season X")
                const hasRealTitle =
                  sn.title && !/^season\s+\d+$/i.test(sn.title);
                const parts: string[] = [base];
                if (hasRealTitle) parts.push(sn.title!);
                const episodeCount =
                  sn.episodeCount ?? sn.episodes?.length ?? null;
                if (episodeCount != null) parts.push(`${episodeCount} 集`);
                return {
                  key: String(sn.seasonNumber),
                  label: parts.join(" · "),
                  posterSrc: sn.posterPath
                    ? posterThumbUrl(sn.posterPath, 92)
                    : undefined,
                };
              })}
              activeTab={String(
                selectedSeason?.seasonNumber ?? seasons[0]?.seasonNumber,
              )}
              onTabChange={(key) => setActiveSeason(Number(key))}
              sticky={false}
            />

            {/* Episode list */}
            {selectedSeason && (
              <div className="space-y-2">
                {(selectedSeason.episodes ?? []).map((ep) => (
                  <EpisodeRow
                    key={ep.id}
                    episode={ep}
                    playMeta={{
                      title: show.title,
                      posterPath: show.posterPath,
                      imdbId: show.imdbId,
                      tmdbId: show.tmdbId,
                    }}
                  />
                ))}
                {(selectedSeason.episodes ?? []).length === 0 && (
                  <p className="py-8 text-center text-sm text-fg-muted">
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
                      src={posterThumbUrl(col.posterPath, 300)}
                      alt={col.name}
                      className="h-12 w-8 flex-shrink-0 rounded object-cover"
                    />
                  )}
                  <p className="truncate text-xs font-medium text-fg-primary">
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
      </div>
    </div>
  );
}
