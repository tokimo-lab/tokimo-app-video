import { useQueryClient } from "@tanstack/react-query";
import {
  type PlayerPlayMeta,
  posterThumbUrl,
  useRuntimeCtx,
} from "@tokimo/sdk";
import { Button, Modal, Spin } from "@tokimo/ui";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type MediaFileOutput } from "../api";
import { WatchHistoryTable } from "../components/WatchHistoryTable";
import {
  useBackgroundArt,
  usePersonEvents,
  usePlayer,
} from "../hooks/shell-stubs";
import { createVideoSourceMetadata } from "../player-source-metadata";
import { useVideoNav } from "../router/useVideoNav";
import {
  CollectionsSection,
  MediaDetailLayout,
  MediaDetailMeta,
  OverviewSection,
} from "./media-detail-layout";
import {
  CastRow,
  CrewRow,
  FilesSection,
  SectionTitle,
} from "./media-detail-shared";

function FavoriteButton({
  isFavorite,
  videoItemId,
}: {
  isFavorite: boolean;
  videoItemId: string;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const toggle = api.video.toggleFavorite.useMutation({
    onSuccess: () =>
      void api.video.getVideoItemDetail.invalidate(qc, { id: videoItemId }),
  });
  return (
    <button
      type="button"
      title={
        isFavorite ? t("media.detail.unfavorite") : t("media.detail.favorite")
      }
      className={`flex h-8 w-8 items-center justify-center rounded-full text-xl transition-transform hover:scale-110 ${isFavorite ? "text-red-500" : "text-fg-muted hover:text-red-400"}`}
      onClick={() => toggle.mutate({ type: "movie", id: videoItemId })}
    >
      {isFavorite ? "♥" : "♡"}
    </button>
  );
}

function formatPosition(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function ResumePromptModal({
  open,
  position,
  onResume,
  onRestart,
  onClose,
}: {
  open: boolean;
  position: number;
  onResume: () => void;
  onRestart: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { shell, windowId } = useRuntimeCtx();
  const [container, setContainer] = useState<HTMLElement | null>(null);

  useEffect(() => {
    setContainer(shell.getWindowContainer(windowId));
  }, [shell, windowId]);

  return (
    <Modal
      open={open}
      closable={false}
      maskClosable
      keyboard
      footer={null}
      width={360}
      centered
      onCancel={onClose}
      container={container}
      styles={{ body: { padding: 0 } }}
    >
      <div className="flex flex-col">
        <button
          type="button"
          className="w-full cursor-pointer border-b border-border-base bg-white/40 px-4 py-4 text-center text-base font-medium text-[var(--color-fg-primary)] transition-colors hover:bg-white/70 dark:bg-white/[0.03] dark:hover:bg-white/[0.08]"
          onClick={onRestart}
        >
          {t("media.detail.restart")}
        </button>
        <button
          type="button"
          className="w-full cursor-pointer bg-white/40 px-4 py-4 text-center text-base font-medium text-[var(--color-fg-primary)] transition-colors hover:bg-white/70 dark:bg-white/[0.03] dark:hover:bg-white/[0.08]"
          onClick={onResume}
        >
          {t("media.detail.resumeFrom", { position: formatPosition(position) })}
        </button>
      </div>
    </Modal>
  );
}

export default function VideoItemDetailPage() {
  const { t } = useTranslation();
  const { params, goBack } = useVideoNav();
  const videoItemId = params.videoItemId;
  const qc = useQueryClient();

  const { data: movie, isLoading } = api.video.getVideoItemDetail.useQuery(
    { id: videoItemId! },
    { enabled: !!videoItemId },
  );

  const { play } = usePlayer();

  const watchHistoryQuery = api.playback.watchHistory.useQuery(
    { videoItemId: videoItemId!, limit: 1 },
    { enabled: !!videoItemId },
  );

  const [resumePrompt, setResumePrompt] = useState<{
    file: MediaFileOutput;
    position: number;
    watchHistoryId?: string;
  } | null>(null);

  const { setBackgroundArt } = useBackgroundArt();
  const artPath = movie?.backdropPath ?? movie?.posterPath;
  useEffect(() => {
    if (artPath) {
      setBackgroundArt(posterThumbUrl(artPath, 1280) ?? null);
    }
    return () => {
      setBackgroundArt(null);
    };
  }, [artPath, setBackgroundArt]);

  // ── WS: refresh movie detail after each person is scraped ──
  usePersonEvents((event) => {
    if (event.videoItemId === videoItemId && videoItemId) {
      api.video.getVideoItemDetail.invalidate(qc, { id: videoItemId });
    }
  });

  const playMeta = useMemo<PlayerPlayMeta>(
    () => ({
      title: movie?.title ?? "",
      poster: movie?.posterPath,
      sourceMetadata: createVideoSourceMetadata({
        videoItemId: movie?.id ?? "",
        imdbId: movie?.imdbId,
        tmdbId: movie?.tmdbId,
      }),
    }),
    [movie?.title, movie?.posterPath, movie?.id, movie?.imdbId, movie?.tmdbId],
  );

  const handleResumePlay = useCallback(
    (fileId: string, position: number, historyId: string) => {
      const file = movie?.files?.find((f) => f.id === fileId);
      if (!file) return;
      play(
        file,
        {
          ...playMeta,
          sourceMetadata: createVideoSourceMetadata({
            ...playMeta.sourceMetadata,
            watchHistoryId: historyId,
          }),
        },
        {
          initialPosition: position,
        },
      );
    },
    [movie?.files, play, playMeta],
  );

  if (isLoading) {
    return (
      <div className="flex h-96 items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!movie) {
    return (
      <div className="flex h-96 flex-col items-center justify-center gap-4">
        <p className="text-fg-muted">{t("media.detail.movieNotFound")}</p>
        <Button onClick={() => goBack()}>{t("media.detail.back")}</Button>
      </div>
    );
  }

  const directors = movie.credits?.filter((c) => c.role === "director") ?? [];
  const writers = movie.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = movie.isFavorite ?? false;
  const isOnlineVideo = !!(
    movie.metadata?.sourceUrl || movie.metadata?.sourceSite
  );

  // First available video file for the big "play" button on the poster
  const firstFile = movie.files?.find((f) => f.videoCodec);

  const handlePlay = (file: NonNullable<typeof firstFile>) => {
    const latest = watchHistoryQuery.data?.[0];
    const pos = latest && !latest.completed ? latest.position : 0;
    if (pos > 10) {
      setResumePrompt({ file, position: pos, watchHistoryId: latest?.id });
    } else {
      play(file, playMeta);
    }
  };

  const yearDisplay =
    movie.releaseDate || movie.year
      ? isOnlineVideo && movie.releaseDate
        ? movie.releaseDate
        : movie.year
      : null;

  return (
    <>
      <ResumePromptModal
        open={resumePrompt !== null}
        position={resumePrompt?.position ?? 0}
        onResume={() => {
          if (resumePrompt) {
            play(
              resumePrompt.file,
              {
                ...playMeta,
                sourceMetadata: createVideoSourceMetadata({
                  ...playMeta.sourceMetadata,
                  watchHistoryId: resumePrompt.watchHistoryId,
                }),
              },
              {
                initialPosition: resumePrompt.position,
              },
            );
          }
          setResumePrompt(null);
        }}
        onRestart={() => {
          if (resumePrompt) {
            play(resumePrompt.file, playMeta);
          }
          setResumePrompt(null);
        }}
        onClose={() => setResumePrompt(null)}
      />
      <MediaDetailLayout
        onBack={goBack}
        title={movie.title}
        posterPath={movie.posterPath}
        posterFallbackEmoji="🎬"
        posterLandscape={isOnlineVideo}
        posterOverlay={
          firstFile ? (
            <button
              type="button"
              aria-label={t("media.detail.play")}
              className="absolute inset-0 flex cursor-pointer items-center justify-center rounded-xl bg-black/30 opacity-0 transition-opacity hover:opacity-100"
              onClick={() => handlePlay(firstFile)}
            >
              <span className="flex h-14 w-14 items-center justify-center rounded-full bg-[var(--color-accent)] shadow-lg">
                <svg
                  className="h-7 w-7 text-white"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                >
                  <path d="M8 5v14l11-7z" />
                </svg>
              </span>
            </button>
          ) : undefined
        }
        headerContent={
          <MediaDetailMeta
            title={movie.title}
            originalTitle={movie.originalTitle}
            tagline={movie.tagline}
            favoriteSlot={
              <FavoriteButton isFavorite={isFavorite} videoItemId={movie.id} />
            }
            yearDisplay={yearDisplay}
            runtime={movie.runtime}
            contentRating={movie.contentRating}
            tmdbRating={movie.tmdbRating}
            imdbRating={movie.imdbRating}
            doubanRating={movie.doubanRating}
            extraBadges={
              movie.scrapedAt ? (
                <span className="inline-flex items-center gap-1 text-xs text-emerald-500">
                  ✨ {t("media.detail.scraped")}
                </span>
              ) : (
                <span className="text-xs text-orange-400">
                  {t("media.detail.notScraped")}
                </span>
              )
            }
            genres={movie.genres}
            tmdbId={movie.tmdbId}
            imdbId={movie.imdbId}
            mediaType="movie"
            directors={directors.map((d) => d.person.name)}
            writers={writers.map((w) => w.person.name)}
            date={isOnlineVideo ? undefined : movie.releaseDate}
            dateLabel={t("media.detail.release")}
            countries={movie.countries}
          >
            {/* Online media metadata (uploader / source) */}
            {movie.metadata?.uploader && (
              <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-fg-muted">
                <span>👤 {movie.metadata.uploader}</span>
                {movie.metadata.sourceSite && (
                  <span>📺 {movie.metadata.sourceSite}</span>
                )}
                {movie.metadata.sourceUrl && (
                  <a
                    href={movie.metadata.sourceUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="truncate text-blue-500 hover:underline"
                    style={{ maxWidth: 300 }}
                  >
                    🔗 {t("media.detail.sourceLink")}
                  </a>
                )}
              </div>
            )}
            {/* Play button */}
            {firstFile && (
              <div className="mt-4 flex items-center gap-3">
                <button
                  type="button"
                  className="flex cursor-pointer items-center gap-2 rounded-lg bg-[var(--color-accent)] px-5 py-2.5 font-semibold text-white hover:opacity-90"
                  onClick={() => handlePlay(firstFile)}
                >
                  <svg
                    className="h-5 w-5"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                  >
                    <path d="M8 5v14l11-7z" />
                  </svg>
                  {t("media.detail.play")}
                </button>
              </div>
            )}
          </MediaDetailMeta>
        }
      >
        <OverviewSection overview={movie.overview} />
        <CollectionsSection collections={movie.collections} />
        <CastRow credits={movie.credits ?? []} />
        <CrewRow credits={movie.credits ?? []} />
        <FilesSection files={movie.files ?? []} playMeta={playMeta} />
        <section className="mb-8">
          <SectionTitle>{t("media.detail.watchHistory.title")}</SectionTitle>
          <WatchHistoryTable
            videoItemId={movie.id}
            onResumePlay={handleResumePlay}
          />
        </section>
      </MediaDetailLayout>
    </>
  );
}
