import { ArrowLeftOutlined, Button, Image, Spin, Tag } from "@acme/components";
import { useEffect, useRef } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useBackgroundArt, useSseEvent } from "../../hooks";
import { trpc } from "../../lib/trpc";
import {
  CastRow,
  CrewRow,
  ExtrasSection,
  FilesSection,
  formatRuntime,
  SectionTitle,
} from "./media-detail-shared";

function FavoriteButton({
  isFavorite,
  movieId,
}: {
  isFavorite: boolean;
  movieId: string;
}) {
  const utils = trpc.useUtils();
  const toggle = trpc.mediaLibrary.toggleFavorite.useMutation({
    onSuccess: () =>
      void utils.mediaLibrary.getMovieDetail.invalidate({ id: movieId }),
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

export default function MovieDetailPage() {
  const { id, movieId } = useParams<{ id: string; movieId: string }>();
  const navigate = useNavigate();
  const utils = trpc.useUtils();

  const { data: movie, isLoading } = trpc.mediaLibrary.getMovieDetail.useQuery(
    { id: movieId! },
    { enabled: !!movieId },
  );

  const { setBackgroundArt } = useBackgroundArt();
  useEffect(() => {
    if (movie?.backdropPath) {
      setBackgroundArt(movie.backdropPath);
    }
    return () => {
      setBackgroundArt(null);
    };
  }, [movie?.backdropPath, setBackgroundArt]);

  // ── Auto-scrape unscraped persons on page load ──
  const scrapeFiredRef = useRef(false);
  const { mutate: scrapePersons } =
    trpc.mediaLibrary.scrapeUnscrapedPersons.useMutation();

  // biome-ignore lint/correctness/useExhaustiveDependencies: scrapeFiredRef is a stable ref intentionally excluded from deps
  useEffect(() => {
    if (!movie || !movieId || scrapeFiredRef.current) return;
    scrapeFiredRef.current = true;
    scrapePersons({ movieId });
  }, [movie, movieId, scrapePersons, scrapeFiredRef]);

  // ── SSE: refresh movie detail after each person is scraped ──
  useSseEvent((event) => {
    if (
      event.type === "person_scraped" &&
      event.movieId === movieId &&
      movieId
    ) {
      utils.mediaLibrary.getMovieDetail.invalidate({ id: movieId });
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
        <Button onClick={() => navigate(`/dashboard/library/${id}`)}>
          返回
        </Button>
      </div>
    );
  }

  const directors = movie.credits?.filter((c) => c.role === "director") ?? [];
  const writers = movie.credits?.filter((c) => c.role === "writer") ?? [];
  const isFavorite = movie.isFavorite ?? false;

  return (
    <div className="-mx-3 -mt-3 -mb-3 relative min-h-full lg:-mx-6 lg:-mt-6 lg:-mb-6">
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
            {movie.posterPath ? (
              <Image
                src={movie.posterPath}
                alt={movie.title}
                className="h-full w-full object-cover"
              />
            ) : (
              <div className="flex aspect-[2/3] items-center justify-center bg-gray-700 text-5xl">
                🎬
              </div>
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h1 className="text-3xl font-bold leading-tight">
                {movie.title}
              </h1>
              <FavoriteButton isFavorite={isFavorite} movieId={movie.id} />
            </div>
            {movie.originalTitle && movie.originalTitle !== movie.title && (
              <p className="mt-0.5 text-sm text-gray-500 dark:text-gray-400">
                {movie.originalTitle}
              </p>
            )}
            {movie.tagline && (
              <p className="mt-1 text-sm italic text-gray-500 dark:text-gray-400">
                {movie.tagline}
              </p>
            )}
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
                <span className="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 dark:border-gray-600 dark:text-gray-300">
                  {movie.contentRating}
                </span>
              )}
              {movie.rating != null && (
                <span className="rounded bg-yellow-500/20 px-2 py-0.5 text-xs font-semibold text-yellow-600 dark:text-yellow-400">
                  ★ {movie.rating.toFixed(1)}
                </span>
              )}
            </div>
            {movie.genres && movie.genres.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1.5">
                {movie.genres.map((g) => (
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
        <div className="mb-8 flex flex-col gap-6 md:flex-row">
          {movie.overview && (
            <div className="flex-1">
              <SectionTitle>简介</SectionTitle>
              <p className="text-sm leading-relaxed text-gray-700 dark:text-gray-300">
                {movie.overview}
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
            {movie.releaseDate && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  发行:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {movie.releaseDate}
                </span>
              </div>
            )}
            {movie.countries && movie.countries.length > 0 && (
              <div>
                <span className="font-semibold text-gray-900 dark:text-gray-100">
                  地区:{" "}
                </span>
                <span className="text-gray-600 dark:text-gray-400">
                  {movie.countries.join(", ")}
                </span>
              </div>
            )}
            {(movie.tmdbId || movie.imdbId) && (
              <div className="flex flex-wrap gap-1.5 pt-1">
                {movie.tmdbId && <Tag color="green">TMDB</Tag>}
                {movie.imdbId && <Tag color="orange">IMDB</Tag>}
              </div>
            )}
          </div>
        </div>

        {/* Collections */}
        {movie.collections && movie.collections.length > 0 && (
          <section className="mb-8">
            <SectionTitle>所属合集</SectionTitle>
            <div className="flex gap-3 overflow-x-auto pb-2">
              {movie.collections.map((col) => (
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

        <CastRow credits={movie.credits ?? []} />
        <CrewRow credits={movie.credits ?? []} />
        <ExtrasSection extras={movie.extras ?? []} />
        <FilesSection files={movie.files ?? []} />
      </div>
    </div>
  );
}
