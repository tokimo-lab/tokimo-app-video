import { useQueryClient } from "@tanstack/react-query";
import { Button, PillTabBar, Spin } from "@tokiomo/components";
import { Play } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/generated/rust-api";
import { posterThumbUrl } from "@/lib/thumb";
import { useBackgroundArt, usePlayer, useWindowNav } from "@/system";
import type { EpisodeOutput } from "@/types";
import { WatchHistoryTable } from "../components/WatchHistoryTable";
import {
  CollectionsSection,
  MediaDetailLayout,
  MediaDetailMeta,
  OverviewSection,
} from "./media-detail-layout";
import {
  CastRow,
  CrewRow,
  formatRuntime,
  MediaFileCard,
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
  const toggle = api.video.toggleFavorite.useMutation({
    onSuccess: () =>
      void api.video.getTvShowDetail.invalidate(qc, { id: tvShowId }),
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
    tvShowId?: string;
    imdbId?: string | null;
    tmdbId?: string | null;
  };
}) {
  const [open, setOpen] = useState(false);
  const { play } = usePlayer();
  const firstFile = episode.files?.[0];
  return (
    <div className="overflow-hidden rounded-lg border border-border-base">
      {/* biome-ignore lint/a11y/noStaticElementInteractions: desktop-only UI, can't nest <button> */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: desktop-only UI */}
      <div
        className="flex w-full cursor-pointer items-stretch gap-4 p-3 transition-colors hover:bg-fill-tertiary/50"
        onClick={() => setOpen((v) => !v)}
      >
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
        <div className="border-t border-border-base p-3">
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
  const { play } = usePlayer();

  const { data: show, isLoading } = api.video.getTvShowDetail.useQuery(
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

  // Build fileId → { episode } map from all loaded seasons/episodes
  const fileMap = useMemo(() => {
    const map = new Map<string, EpisodeOutput>();
    for (const season of show?.seasons ?? []) {
      for (const ep of season.episodes ?? []) {
        for (const f of ep.files ?? []) {
          map.set(f.id, ep);
        }
      }
    }
    return map;
  }, [show?.seasons]);

  const handleResumePlay = useCallback(
    (fileId: string, position: number, historyId: string) => {
      if (!show) return;
      const episode = fileMap.get(fileId);
      if (!episode) return;
      const file = episode.files?.find((f) => f.id === fileId);
      if (!file) return;
      void play(
        file,
        {
          title: episode.title ?? `第 ${episode.episodeNumber} 集`,
          posterPath: show.posterPath,
          tvShowId: show.id,
          episodeId: episode.id,
          imdbId: show.imdbId,
          tmdbId: show.tmdbId,
        },
        { initialPosition: position, watchHistoryId: historyId },
      );
    },
    [fileMap, play, show],
  );

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
        <p className="text-fg-muted">未找到该剧集</p>
        <Button onClick={() => goBack()}>返回</Button>
      </div>
    );
  }

  const directors = show.credits?.filter((c) => c.role === "director") ?? [];
  const writers = show.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = show.isFavorite ?? false;

  return (
    <MediaDetailLayout
      onBack={goBack}
      title={show.title}
      posterPath={show.posterPath}
      posterFallbackEmoji="📺"
      headerContent={
        <MediaDetailMeta
          title={show.title}
          originalTitle={show.originalTitle}
          favoriteSlot={
            <FavoriteButton isFavorite={isFavorite} tvShowId={show.id} />
          }
          yearDisplay={show.year}
          contentRating={show.contentRating}
          tmdbRating={show.tmdbRating}
          imdbRating={show.imdbRating}
          doubanRating={show.doubanRating}
          extraBadges={
            <>
              {seasons.length > 0 && (
                <span className="text-fg-secondary">· {seasons.length} 季</span>
              )}
              {show.status && (
                <span
                  className={`rounded px-1.5 py-0.5 text-xs font-medium ${
                    show.status === "ended"
                      ? "bg-fill-tertiary text-fg-secondary"
                      : "bg-green-100/60 text-green-700 dark:bg-green-600/60 dark:text-green-200"
                  }`}
                >
                  {show.status}
                </span>
              )}
            </>
          }
          genres={show.genres}
          tmdbId={show.tmdbId}
          imdbId={show.imdbId}
          tvdbId={show.tvdbId}
          mediaType="tv"
          directors={directors.map((d) => d.person.name)}
          writers={writers.map((w) => w.person.name)}
          date={show.firstAirDate}
          dateLabel="首播"
          countries={show.countries}
        />
      }
    >
      <OverviewSection overview={show.overview} />

      {/* Season + Episodes */}
      {seasons.length > 0 && (
        <section className="mb-8">
          <PillTabBar
            tabs={seasons.map((sn) => {
              const base = `第 ${sn.seasonNumber} 季`;
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

          {selectedSeason && (
            <div className="space-y-2">
              {(selectedSeason.episodes ?? []).map((ep) => (
                <EpisodeRow
                  key={ep.id}
                  episode={ep}
                  playMeta={{
                    title: show.title,
                    posterPath: show.posterPath,
                    tvShowId: show.id,
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

      <CollectionsSection collections={show.collections} />
      <CastRow credits={show.credits ?? []} />
      <CrewRow credits={show.credits ?? []} />

      <section className="mb-8">
        <SectionTitle>观看记录</SectionTitle>
        <WatchHistoryTable tvShowId={show.id} onResumePlay={handleResumePlay} />
      </section>
    </MediaDetailLayout>
  );
}
