import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeftOutlined, Button, Modal, Spin } from "@tokiomo/components";
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import type { MediaFileOutput } from "@/types";
import { WatchHistoryTable } from "../../components/player/WatchHistoryTable";
import { usePlayer } from "../../contexts/PlayerContext";
import { api } from "../../generated/rust-api";
import { useBackgroundArt, useSseEvent } from "../../hooks";
import { resolveStoragePath } from "../../lib/storage-url";
import {
  CastRow,
  CrewRow,
  ExtrasSection,
  FilesSection,
  formatRuntime,
  MediaInfoBlock,
  MediaPoster,
  MediaTagsRow,
  SectionTitle,
} from "./media-detail-shared";

function WatchHistorySection({ movieId }: { movieId: string }) {
  return (
    <section className="mb-8">
      <SectionTitle>观看记录</SectionTitle>
      <WatchHistoryTable movieId={movieId} />
    </section>
  );
}

function FavoriteButton({
  isFavorite,
  movieId,
}: {
  isFavorite: boolean;
  movieId: string;
}) {
  const qc = useQueryClient();
  const toggle = api.app.toggleFavorite.useMutation({
    onSuccess: () =>
      void api.app.getMovieDetail.invalidate(qc, { id: movieId }),
  });
  return (
    <button
      type="button"
      title={isFavorite ? "取消收藏" : "收藏"}
      className={`flex h-8 w-8 items-center justify-center rounded-full text-xl transition-transform hover:scale-110 ${isFavorite ? "text-red-500" : "text-gray-400 hover:text-red-400"}`}
      onClick={() => toggle.mutate({ type: "movie", id: movieId })}
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
          className="w-full cursor-pointer border-b border-[var(--glass-border)] bg-white/40 px-4 py-4 text-center text-base font-medium text-[var(--text-primary)] transition-colors hover:bg-white/70 dark:bg-white/[0.03] dark:hover:bg-white/[0.08]"
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

export default function MovieDetailPage() {
  const { id, movieId } = useParams<{ id: string; movieId: string }>();
  const navigate = useNavigate();
  const qc = useQueryClient();

  const { data: movie, isLoading } = api.app.getMovieDetail.useQuery(
    { id: movieId! },
    { enabled: !!movieId },
  );

  const { play } = usePlayer();

  const resumeQuery = api.playback.resumePosition.useQuery(
    { movieId: movieId! },
    { enabled: !!movieId },
  );

  const [resumePrompt, setResumePrompt] = useState<{
    file: MediaFileOutput;
    position: number;
  } | null>(null);

  const { setBackgroundArt } = useBackgroundArt();
  useEffect(() => {
    if (movie?.backdropPath) {
      setBackgroundArt(resolveStoragePath(movie.backdropPath));
    }
    return () => {
      setBackgroundArt(null);
    };
  }, [movie?.backdropPath, setBackgroundArt]);

  // ── SSE: refresh movie detail after each person is scraped ──
  useSseEvent((event) => {
    if (
      event.type === "person_scraped" &&
      event.movieId === movieId &&
      movieId
    ) {
      api.app.getMovieDetail.invalidate(qc, { id: movieId });
    }
  });

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
        <p className="text-gray-500">未找到该电影</p>
        <Button onClick={() => navigate(`/dashboard/app/${id}`)}>返回</Button>
      </div>
    );
  }

  const directors = movie.credits?.filter((c) => c.role === "director") ?? [];
  const writers = movie.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = movie.isFavorite ?? false;

  // First available video file for the big "play" button on the poster
  const firstFile = movie.files?.find((f) => f.videoCodec);

  const playMeta = {
    title: movie.title,
    posterPath: movie.posterPath,
    movieId: movie.id,
    imdbId: movie.imdbId,
    tmdbId: movie.tmdbId,
  };

  const handlePlay = (file: NonNullable<typeof firstFile>) => {
    const pos = resumeQuery.data?.position ?? 0;
    if (pos > 10) {
      setResumePrompt({ file, position: pos });
    } else {
      play(file, playMeta);
    }
  };

  return (
    <div className="-mx-3 -mt-3 -mb-3 relative min-h-full lg:-mx-6 lg:-mt-6 lg:-mb-6">
      {/* ── Resume prompt ── */}
      <ResumePromptModal
        open={resumePrompt !== null}
        position={resumePrompt?.position ?? 0}
        onResume={() => {
          if (resumePrompt) {
            play(resumePrompt.file, playMeta, {
              initialPosition: resumePrompt.position,
            });
          }
          setResumePrompt(null);
        }}
        onRestart={() => {
          if (resumePrompt) {
            play(resumePrompt.file, playMeta, { initialPosition: 0 });
          }
          setResumePrompt(null);
        }}
        onClose={() => setResumePrompt(null)}
      />
      {/* ── Header ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        <div className="mb-6">
          <Button
            icon={<ArrowLeftOutlined />}
            onClick={() => navigate(`/dashboard/app/${id}`)}
          >
            返回
          </Button>
        </div>
        <div className="flex items-start gap-6">
          {/* Poster with play overlay */}
          <div
            className="relative hidden flex-shrink-0 md:block"
            style={{ width: 160 }}
          >
            <MediaPoster
              posterPath={movie.posterPath}
              title={movie.title}
              fallbackEmoji="🎬"
            />
            {firstFile && (
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
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h1 className="text-3xl font-bold leading-tight">
                {movie.title}
              </h1>
              <FavoriteButton isFavorite={isFavorite} movieId={movie.id} />
            </div>
            {(movie.originalTitle && movie.originalTitle !== movie.title) ||
            movie.tagline ? (
              <p className="mt-0.5 truncate text-sm text-gray-500 dark:text-gray-400">
                {movie.originalTitle && movie.originalTitle !== movie.title
                  ? movie.originalTitle
                  : null}
                {movie.originalTitle &&
                  movie.originalTitle !== movie.title &&
                  movie.tagline && <span className="mx-1">·</span>}
                {movie.tagline ? (
                  <span className="italic">{movie.tagline}</span>
                ) : null}
              </p>
            ) : null}
            <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
              {movie.year && (
                <span className="text-gray-600 dark:text-gray-300">
                  {movie.year}
                </span>
              )}
              {movie.runtime != null && (
                <span className="text-gray-600 dark:text-gray-300">
                  · {formatRuntime(movie.runtime)}
                </span>
              )}
              {movie.contentRating && (
                <span className="rounded border border-[var(--glass-border)] px-1.5 py-0.5 text-xs text-gray-600 dark:text-gray-300">
                  {movie.contentRating}
                </span>
              )}
              {movie.tmdbRating != null && (
                <span className="rounded bg-yellow-500/20 px-2 py-0.5 text-xs font-semibold text-yellow-600 dark:text-yellow-400">
                  TMDB ★ {movie.tmdbRating.toFixed(1)}
                </span>
              )}
              {movie.imdbRating != null && (
                <span className="rounded bg-amber-500/20 px-2 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400">
                  IMDb ★ {movie.imdbRating.toFixed(1)}
                </span>
              )}
              {movie.doubanRating != null && (
                <span className="rounded bg-green-500/20 px-2 py-0.5 text-xs font-semibold text-green-600 dark:text-green-400">
                  豆瓣 ★ {movie.doubanRating.toFixed(1)}
                </span>
              )}
              <MediaTagsRow
                genres={movie.genres}
                tmdbId={movie.tmdbId}
                imdbId={movie.imdbId}
                mediaType="movie"
              />
            </div>
            <MediaInfoBlock
              directors={directors.map((d) => d.person.name)}
              writers={writers.map((w) => w.person.name)}
              date={movie.releaseDate}
              dateLabel="发行"
              countries={movie.countries}
            />
            {/* Play button row */}
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
          </div>
        </div>
      </div>

      {/* ── Body ── */}
      <div className="relative z-10 px-6 pt-6 pb-6">
        {movie.overview && (
          <div className="mb-8">
            <SectionTitle>简介</SectionTitle>
            <p className="text-sm leading-relaxed text-gray-700 dark:text-gray-300">
              {movie.overview}
            </p>
          </div>
        )}

        {/* Collections */}
        {movie.collections && movie.collections.length > 0 && (
          <section className="mb-8">
            <SectionTitle>所属合集</SectionTitle>
            <div className="flex gap-3 overflow-x-auto pb-2">
              {movie.collections.map((col) => (
                <div
                  key={col.id}
                  className="flex w-[200px] flex-shrink-0 items-center gap-2.5 rounded-lg border border-[var(--glass-border)] p-2"
                >
                  {col.posterPath && (
                    <img
                      src={resolveStoragePath(col.posterPath)}
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

        <CastRow credits={movie.credits ?? []} />
        <CrewRow credits={movie.credits ?? []} />
        <ExtrasSection extras={movie.extras ?? []} />
        <FilesSection files={movie.files ?? []} playMeta={playMeta} />
        <WatchHistorySection movieId={movie.id} />
      </div>
    </div>
  );
}
