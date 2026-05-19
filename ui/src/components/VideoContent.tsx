import { cn, Empty, PosterCard, Spin } from "@tokimo/ui";
import { getGenreName } from "@tokiomo/types";
import { motion } from "framer-motion";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ContentSearch } from "@/components/media/ContentSearch";
import type { VideoOutput } from "@/generated/rust-api";
import { api } from "@/generated/rust-api";
import { posterThumbUrl } from "@/lib/thumb";
import { useInfiniteScroll } from "@/shared/hooks/use-infinite-scroll";
import { useLang, useWindowNav } from "@/system";
import type { TvShowOutput, VideoItemOutput } from "@/types";
import type { FilterOption, MediaFilters } from "./MediaFilterPanel";
import MediaFilterPanel, {
  EMPTY_FILTERS,
  getCountryDisplayName,
} from "./MediaFilterPanel";

type MediaItem = (VideoItemOutput | TvShowOutput) & {
  posterPath?: string | null;
};

const MIN_CARD_WIDTH = 150;
const CARD_GAP = 12;
const CARD_TITLE_HEIGHT = 52;

const POSTER_BADGE_CLASS =
  "absolute right-0 inline-flex items-center gap-1 rounded-l-md rounded-r-none border border-r-0 border-white/12 bg-[var(--sidebar-bg)] px-2 py-1 text-[10px] font-medium shadow-sm backdrop-blur-md";

const LAYOUT_SPRING = {
  type: "spring" as const,
  stiffness: 400,
  damping: 30,
  mass: 0.8,
};

function MediaCard({
  item,
  onClick,
  landscape,
}: {
  item: MediaItem;
  onClick: () => void;
  landscape?: boolean;
}) {
  return (
    <PosterCard
      src={posterThumbUrl(item.posterPath, 300)}
      alt={item.title}
      landscape={landscape}
      badges={
        <>
          {item.year && (
            <span className={`${POSTER_BADGE_CLASS} bottom-2 text-white`}>
              {item.year}
            </span>
          )}
          {item.rating != null && (
            <span className={`${POSTER_BADGE_CLASS} top-2 text-amber-400`}>
              <span>★</span>
              <span>{item.rating.toFixed(1)}</span>
            </span>
          )}
          {(item as VideoItemOutput).isFavorite && (
            <span className="absolute top-1 left-1 text-base text-red-500">
              ♥
            </span>
          )}
          {"scrapedAt" in item && !(item as VideoItemOutput).scrapedAt && (
            <span
              className="absolute top-1.5 right-1.5 h-2 w-2 rounded-full bg-orange-400 ring-1 ring-black/30"
              title="未刮削"
            />
          )}
        </>
      }
      onClick={onClick}
    >
      <p
        className={cn(
          "truncate text-sm font-medium",
          (item as VideoItemOutput).isFavorite
            ? "text-[var(--accent)]"
            : "text-fg-primary",
        )}
        title={item.title}
      >
        {item.title}
      </p>
      {(() => {
        const date =
          "releaseDate" in item
            ? (item as VideoItemOutput).releaseDate
            : (item as TvShowOutput).firstAirDate;
        return date ? (
          <p className="truncate text-xs text-fg-muted">{date}</p>
        ) : null;
      })()}
    </PosterCard>
  );
}

// Sort options moved to MediaFilterPanel

function parseSortValue(v: string) {
  if (v === "title_asc") return { sortBy: "title", sortDir: "asc" };
  if (v === "title_desc") return { sortBy: "title", sortDir: "desc" };
  if (v === "year_desc") return { sortBy: "year", sortDir: "desc" };
  if (v === "year_asc") return { sortBy: "year", sortDir: "asc" };
  if (v === "rating") return { sortBy: "rating", sortDir: "desc" };
  return { sortBy: "addedAt", sortDir: "desc" };
}

export default function VideoContent({
  category,
  syncing,
}: {
  category: VideoOutput;
  syncing?: boolean;
}) {
  const { navigate } = useWindowNav();
  const { lang } = useLang();
  const id = category.id;
  const libType = category.type;
  const isTv = libType === "tv" || libType === "anime";
  const isLandscape = libType === "online_video";

  const [page, setPage] = useState(1);
  const [filters, setFilters] = useState<MediaFilters>(EMPTY_FILTERS);

  const gridWrapperRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  useEffect(() => {
    const el = gridWrapperRef.current;
    if (!el) return;
    setContainerWidth(el.getBoundingClientRect().width);
    const ro = new ResizeObserver((entries) => {
      setContainerWidth(entries[0].contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const minCardWidth = isLandscape ? 260 : MIN_CARD_WIDTH;
  const cols = useMemo(
    () =>
      containerWidth > 0
        ? Math.max(
            2,
            Math.floor((containerWidth + CARD_GAP) / (minCardWidth + CARD_GAP)),
          )
        : isLandscape
          ? 3
          : 4,
    [containerWidth, minCardWidth, isLandscape],
  );

  const pageSize = useMemo(() => {
    const estimatedCols = Math.max(
      2,
      Math.floor(
        (window.innerWidth * 0.7 + CARD_GAP) / (MIN_CARD_WIDTH + CARD_GAP),
      ),
    );
    const cardWidth = (window.innerWidth * 0.7) / estimatedCols;
    const rowHeight = Math.round(cardWidth * 1.5) + CARD_TITLE_HEIGHT;
    const visibleRows = Math.ceil(window.innerHeight / (rowHeight + CARD_GAP));
    return Math.max(estimatedCols * (visibleRows + 6), 24);
  }, []);

  const sortParams = parseSortValue(filters.sortBy || "addedAt");

  const genresQuery = api.video.listGenres.useQuery({ id }, { enabled: !!id });
  const genres = genresQuery.data ?? [];

  const countriesQuery = api.video.listCountries.useQuery(
    { id },
    { enabled: !!id },
  );
  const countries = countriesQuery.data ?? [];

  const moviesQuery = api.video.listVideoItems.useQuery(
    {
      id,
      page,
      pageSize,
      ...sortParams,
      genreId: filters.genreId || undefined,
      country: filters.country || undefined,
      favorite: filters.favorite === "true" ? true : undefined,
      resolution: filters.resolution || undefined,
      runtime: filters.runtime || undefined,
    },
    { enabled: !!id && !isTv && pageSize > 0 },
  );

  const tvQuery = api.video.listTvShows.useQuery(
    {
      id,
      page,
      pageSize,
      ...sortParams,
      genreId: filters.genreId || undefined,
      country: filters.country || undefined,
      favorite: filters.favorite === "true" ? true : undefined,
      resolution: filters.resolution || undefined,
    },
    { enabled: !!id && isTv && pageSize > 0 },
  );

  const paginatedQuery = isTv ? tvQuery : moviesQuery;

  const { items, total, hasMore, sentinelRef, reset } =
    useInfiniteScroll<MediaItem>({
      queryData: paginatedQuery.data as
        | { items: MediaItem[]; total: number; page: number }
        | undefined,
      isFetching: paginatedQuery.isFetching,
      onLoadMore: () => setPage((p) => p + 1),
      enabled: !syncing,
    });

  const resetAll = useCallback(() => {
    reset();
    setPage(1);
  }, [reset]);

  const isLoading =
    paginatedQuery.isLoading ||
    (items.length === 0 && paginatedQuery.isFetching);

  // Reset when switching category
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally reset on id change
  useEffect(() => {
    resetAll();
    setFilters(EMPTY_FILTERS);
  }, [id]);

  const handleItemClick = useCallback(
    (item: MediaItem) => {
      if (isTv) {
        navigate(`/tv/${item.id}`, `TokimoVideo · ${item.title ?? "TV Show"}`);
      } else {
        navigate(
          `/movies/${item.id}`,
          `TokimoVideo · ${item.title ?? "Movie"}`,
        );
      }
    },
    [isTv, navigate],
  );

  const handleFiltersChange = useCallback(
    (next: MediaFilters) => {
      setFilters(next);
      resetAll();
    },
    [resetAll],
  );

  const activeFilterCount = useMemo(() => {
    let c = 0;
    if (filters.sortBy && filters.sortBy !== "addedAt") c++;
    if (filters.genreId) c++;
    if (filters.country) c++;
    if (filters.runtime) c++;
    if (filters.favorite) c++;
    if (filters.resolution) c++;
    return c;
  }, [filters]);

  const genreOptions: FilterOption[] = useMemo(
    () =>
      genres.map((g) => ({
        label: getGenreName(g.tmdbGenreId, lang) || g.name,
        value: g.id,
      })),
    [genres, lang],
  );

  const countryOptions: FilterOption[] = useMemo(
    () => countries.map((c) => ({ label: getCountryDisplayName(c), value: c })),
    [countries],
  );

  return (
    <div className="flex h-full flex-col overflow-y-auto p-4">
      {/* Search bar — PillTabBar style, sticky */}
      <div className="sticky top-0 z-10 -mx-4 -mt-4 mb-0 bg-[var(--bg-primary)] px-4 pt-4 pb-3">
        <ContentSearch
          appId={id}
          searchType={isTv ? "tv" : "movie"}
          placeholder={isTv ? "搜索电视剧…" : "搜索影片…"}
          onSelect={(item) => {
            if (isTv) {
              navigate(
                `/tv/${item.id}`,
                `TokimoVideo · ${item.title ?? "TV Show"}`,
              );
            } else {
              navigate(
                `/movies/${item.id}`,
                `TokimoVideo · ${item.title ?? "Movie"}`,
              );
            }
          }}
        />
      </div>

      {/* Filter Panel - always visible */}
      <div className="rounded-lg border border-white/8 bg-black/20 px-4 py-3 backdrop-blur-md">
        <MediaFilterPanel
          filters={filters}
          onChange={handleFiltersChange}
          genreOptions={genreOptions}
          countryOptions={countryOptions}
          showRuntime={!isTv}
        />
      </div>

      <div ref={gridWrapperRef} className="mt-3 min-h-0 flex-1">
        {(isLoading || syncing) && items.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Spin />
          </div>
        ) : items.length === 0 ? (
          <Empty
            className="flex h-full items-center justify-center"
            description={
              activeFilterCount > 0
                ? "该筛选条件下暂无资源"
                : "暂无资源，请先同步"
            }
          />
        ) : (
          <>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
                gap: CARD_GAP,
              }}
            >
              {items.map((item) => (
                <motion.div key={item.id} layout transition={LAYOUT_SPRING}>
                  <MediaCard
                    item={item}
                    landscape={isLandscape}
                    onClick={() => handleItemClick(item)}
                  />
                </motion.div>
              ))}
            </div>

            <div ref={sentinelRef} className="h-px" />
            <div className="mt-2 flex justify-center py-3">
              {paginatedQuery.isFetching && <Spin />}
              {!hasMore && total > 0 && !paginatedQuery.isFetching && (
                <p className="text-xs text-fg-muted">已加载全部 {total} 个</p>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
