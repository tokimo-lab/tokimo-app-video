import type { QueryClient } from "@tanstack/react-query";
import {
  type AppRuntimeCtx,
  type PlayerExtension,
  useRuntimeCtx,
} from "@tokimo/sdk";
import { api, type EpisodeOutput } from "./api";
import { EpisodeListMenu } from "./components/EpisodeListMenu";
import {
  createVideoSourceMetadata,
  getVideoSourceMetadata,
} from "./player-source-metadata";
import { withProviders } from "./shared/providers";

interface EpisodeWithSeason extends EpisodeOutput {
  seasonNumber: number;
}

function flattenEpisodes(
  tvShow: Awaited<ReturnType<typeof api.video.getTvShowDetail.fetch>>,
): EpisodeWithSeason[] {
  return (
    tvShow.seasons?.flatMap((season) =>
      (season.episodes ?? []).map((episode) => ({
        ...episode,
        seasonNumber: season.seasonNumber,
      })),
    ) ?? []
  );
}

function firstPlayableFile(episode: EpisodeWithSeason) {
  return episode.files?.[0] ?? null;
}

function getFileDuration(file: unknown): number | null {
  if (typeof file !== "object" || file === null || !("duration" in file)) {
    return null;
  }
  const duration = (file as { duration?: unknown }).duration;
  return typeof duration === "number" && Number.isFinite(duration)
    ? duration
    : null;
}

function TaskbarNavigateButton({
  sourceMetadata,
}: {
  sourceMetadata?: Record<string, unknown>;
}) {
  const { shell } = useRuntimeCtx();
  const meta = getVideoSourceMetadata(sourceMetadata);
  if (!meta?.videoItemId && !meta?.tvShowId) return null;
  const route = meta.videoItemId
    ? `/movies/${meta.videoItemId}`
    : `/tv/${meta.tvShowId}`;
  return (
    <button
      type="button"
      className="cursor-pointer rounded px-2 py-0.5 text-[11px] text-fg-secondary hover:bg-white/10"
      onClick={() => shell.windowNav.navigate(route)}
    >
      详情
    </button>
  );
}

export function createVideoPlayerExtension(
  ctx: AppRuntimeCtx,
  queryClient: QueryClient,
): PlayerExtension {
  return {
    async getResumePosition(_file, sourceMetadata) {
      const meta = getVideoSourceMetadata(sourceMetadata);
      if (!meta) return null;
      const history = await api.playback.watchHistory.fetch({
        videoItemId: meta.videoItemId,
        episodeId: meta.episodeId,
        limit: 1,
      });
      const latest = history[0];
      if (!latest || latest.completed || latest.position <= 10) return null;
      return latest.position;
    },

    async getNextItem(_file, sourceMetadata) {
      const meta = getVideoSourceMetadata(sourceMetadata);
      if (!meta?.tvShowId || !meta.episodeId) return null;
      const input = { id: meta.tvShowId };
      const cached = queryClient.getQueryData<
        Awaited<ReturnType<typeof api.video.getTvShowDetail.fetch>>
      >(api.video.getTvShowDetail.queryKey(input));
      const tvShow = cached ?? (await api.video.getTvShowDetail.fetch(input));
      const episodes = flattenEpisodes(tvShow);
      const currentIndex = episodes.findIndex(
        (episode) => episode.id === meta.episodeId,
      );
      if (currentIndex < 0) return null;
      const nextEpisode = episodes
        .slice(currentIndex + 1)
        .find((episode) => firstPlayableFile(episode) !== null);
      if (!nextEpisode) return null;
      const nextFile = firstPlayableFile(nextEpisode);
      if (!nextFile) return null;
      return {
        file: nextFile,
        meta: {
          title: nextEpisode.title ?? `Episode ${nextEpisode.episodeNumber}`,
          poster: tvShow.posterPath,
          sourceMetadata: createVideoSourceMetadata({
            tvShowId: tvShow.id,
            episodeId: nextEpisode.id,
            imdbId: tvShow.imdbId,
            tmdbId: tvShow.tmdbId,
          }),
        },
      };
    },

    onProgress(file, position, sourceMetadata) {
      const meta = getVideoSourceMetadata(sourceMetadata);
      const watchHistoryId = meta?.watchHistoryId;
      if (!watchHistoryId) return;
      const duration = getFileDuration(file);
      if (duration === null) return;
      void api.playback.reportProgress
        .mutate({ watchHistoryId, position, duration })
        .catch((error) => {
          console.error("[VideoPlayerExtension] reportProgress failed:", error);
        });
    },

    renderTaskbarActions(_file, sourceMetadata) {
      return withProviders(
        ctx,
        queryClient,
        <TaskbarNavigateButton sourceMetadata={sourceMetadata} />,
      );
    },

    renderEpisodePicker(_file, sourceMetadata) {
      const meta = getVideoSourceMetadata(sourceMetadata);
      if (!meta?.tvShowId || !meta.episodeId) return null;
      return withProviders(ctx, queryClient, <EpisodeListMenu />);
    },
  };
}
