import { useQueryClient } from "@tanstack/react-query";
import { Button, Modal, Spin } from "@tokimo/ui";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "./shell-shim/api";
import { posterThumbUrl } from "../shell-shim/lib";
import {
  useAppEvent,
  useBackgroundArt,
  usePlayer,
  useWindowNav,
} from "../shell-shim/system";
import type { MediaFileOutput } from "../shell-shim/types";
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
  const qc = useQueryClient();
  const toggle = api.video.toggleFavorite.useMutation({
    onSuccess: () =>
      void api.video.getVideoItemDetail.invalidate(qc, { id: videoItemId }),
  });
  return (
    <button
      type="button"
      title={isFavorite ? "取消收藏" : "收藏"}
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
      styles={{ body: { padding: 0 } }}
    >
      <div className="flex flex-col">
        <button
          type="button"
          className="w-full cursor-pointer border-b border-border-base bg-white/40 px-4 py-4 text-center text-base font-medium text-[var(--text-primary)] transition-colors hover:bg-white/70 dark:bg-white/[0.03] dark:hover:bg-white/[0.08]"
          onClick={onRestart}
        >
          从头开始
        </button>
        <button
          type="button"
          className="w-full cursor-pointer bg-white/40 px-4 py-4 text-center text-base font-medium text-[var(--text-primary)] transition-colors hover:bg-white/70 dark:bg-white/[0.03] dark:hover:bg-white/[0.08]"
          onClick={onResume}
        >
          从 {formatPosition(position)} 继续
        </button>
      </div>
    </Modal>
  );
}

export default function VideoItemDetailPage() {
  const { params, goBack } = useWindowNav();
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
  useAppEvent((event) => {
    if (
      event.type === "person_scraped" &&
      event.videoItemId === videoItemId &&
      videoItemId
    ) {
      api.video.getVideoItemDetail.invalidate(qc, { id: videoItemId });
    }
  });

  const playMeta = useMemo(
    () => ({
      title: movie?.title ?? "",
      posterPath: movie?.posterPath,
      videoItemId: movie?.id ?? "",
      imdbId: movie?.imdbId,
      tmdbId: movie?.tmdbId,
    }),
    [movie?.title, movie?.posterPath, movie?.id, movie?.imdbId, movie?.tmdbId],
  );

  const handleResumePlay = useCallback(
    (fileId: string, position: number, historyId: string) => {
      const file = movie?.files?.find((f) => f.id === fileId);
      if (!file) return;
      play(file, playMeta, {
        initialPosition: position,
        watchHistoryId: historyId,
      });
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
        <p className="text-fg-muted">未找到该电影</p>
        <Button onClick={() => goBack()}>返回</Button>
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
            play(resumePrompt.file, playMeta, {
              initialPosition: resumePrompt.position,
              watchHistoryId: resumePrompt.watchHistoryId,
            });
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
              aria-label="播放"
              className="absolute inset-0 flex cursor-pointer items-center justify-center rounded-xl bg-black/30 opacity-0 transition-opacity hover:opacity-100"
              onClick={() => handlePlay(firstFile)}
            >
              <span className="flex h-14 w-14 items-center justify-center rounded-full bg-[var(--accent)] shadow-lg">
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
                  ✨ 已刮削
                </span>
              ) : (
                <span className="text-xs text-orange-400">未刮削</span>
              )
            }
            genres={movie.genres}
            tmdbId={movie.tmdbId}
            imdbId={movie.imdbId}
            mediaType="movie"
            directors={directors.map((d) => d.person.name)}
            writers={writers.map((w) => w.person.name)}
            date={isOnlineVideo ? undefined : movie.releaseDate}
            dateLabel="发行"
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
                    🔗 源链接
                  </a>
                )}
              </div>
            )}
            {/* Play button */}
            {firstFile && (
              <div className="mt-4 flex items-center gap-3">
                <button
                  type="button"
                  className="flex cursor-pointer items-center gap-2 rounded-lg bg-[var(--accent)] px-5 py-2.5 font-semibold text-white hover:opacity-90"
                  onClick={() => handlePlay(firstFile)}
                >
                  <svg
                    className="h-5 w-5"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                  >
                    <path d="M8 5v14l11-7z" />
                  </svg>
                  播放
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
          <SectionTitle>观看记录</SectionTitle>
          <WatchHistoryTable
            videoItemId={movie.id}
            onResumePlay={handleResumePlay}
          />
        </section>
      </MediaDetailLayout>
    </>
  );
}
