import { posterThumbUrl, useInfiniteScroll } from "@tokimo/sdk";
import { cn, Empty, Input, PosterCard, Spin } from "@tokimo/ui";
import { motion } from "framer-motion";
import { Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  getGenreKey,
  type TvShowOutput,
  type VideoItemOutput,
  type VideoOutput,
} from "../api";
import { useBrowseViewport } from "../hooks/useBrowseViewport";
import { useVideoNav } from "../router/useVideoNav";
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
  "absolute right-0 inline-flex items-center gap-1 rounded-l-md rounded-r-none border border-r-0 border-white/12 bg-[var(--color-surface-sidebar)] px-2 py-1 text-[10px] font-medium shadow-sm backdrop-blur-md";

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
  const { t } = useTranslation();
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
              title={t("media.detail.notScraped")}
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
            ? "text-[var(--color-accent)]"
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
  active = true,
}: {
  category: VideoOutput;
  active?: boolean;
  syncing?: boolean;
}) {
  const { navigate } = useVideoNav();
  const { t } = useTranslation();
  const id = category.id;
  const libType = category.type;
  const isTv = libType === "tv" || libType === "anime";
  const isLandscape = libType === "online_video";

  const [page, setPage] = useState(1);
  const [filters, setFilters] = useState<MediaFilters>(EMPTY_FILTERS);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [composing, setComposing] = useState(false);
  const {
    scrollRef,
    gridWrapperRef,
    width: containerWidth,
    rememberCard,
    resetScroll,
  } = useBrowseViewport(active);

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
      search: search || undefined,
      ...sortParams,
      genreId: filters.genreId || undefined,
      country: filters.country || undefined,
      favorite: filters.favorite === "true" ? true : undefined,
      resolution: filters.resolution || undefined,
      runtime: filters.runtime || undefined,
    },
    { enabled: active && !!id && !isTv && pageSize > 0 },
  );

  const tvQuery = api.video.listTvShows.useQuery(
    {
      id,
      page,
      pageSize,
      search: search || undefined,
      ...sortParams,
      genreId: filters.genreId || undefined,
      country: filters.country || undefined,
      favorite: filters.favorite === "true" ? true : undefined,
      resolution: filters.resolution || undefined,
    },
    { enabled: active && !!id && isTv && pageSize > 0 },
  );

  const paginatedQuery = isTv ? tvQuery : moviesQuery;

  const { items, total, hasMore, sentinelRef, reset } =
    useInfiniteScroll<MediaItem>({
      queryData: paginatedQuery.data,
      isFetching: paginatedQuery.isFetching,
      onLoadMore: () => setPage((p) => p + 1),
      enabled: active && !paginatedQuery.isError,
    });

  const visibleSentinelRef = useCallback(
    (node: HTMLDivElement | null) => sentinelRef(active ? node : null),
    [active, sentinelRef],
  );

  const resetAll = useCallback(() => {
    reset();
    setPage(1);
    resetScroll();
  }, [reset, resetScroll]);

  useEffect(() => {
    if (composing || !active || searchInput.trim() === search) return;
    const timer = setTimeout(() => {
      setSearch(searchInput.trim());
      resetAll();
    }, 300);
    return () => clearTimeout(timer);
  }, [searchInput, search, composing, active, resetAll]);

  const isLoading =
    paginatedQuery.isLoading ||
    (items.length === 0 && paginatedQuery.isFetching);

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
      genres.map((g) => {
        const genreKey = getGenreKey(g.tmdbGenreId);
        return {
          label: genreKey ? t(genreKey) : g.name,
          value: g.id,
        };
      }),
    [genres, t],
  );

  const countryOptions: FilterOption[] = useMemo(
    () =>
      countries.map((c) => {
        const countryKey = getCountryDisplayName(c);
        return { label: t(countryKey), value: c };
      }),
    [countries, t],
  );

  return (
    <div
      ref={scrollRef}
      data-video-scroll
      className="relative flex h-full flex-col overflow-y-auto p-4"
    >
      {/* Search bar */}
      <div className="-mx-4 -mt-4 mb-0 bg-surface-base px-4 pt-4 pb-3">
        <Input
          className="w-full"
          aria-label={t("media.sidebar.searchLibrary", { name: category.name })}
          prefix={<Search />}
          value={searchInput}
          onChange={(event) => setSearchInput(event.target.value)}
          onCompositionStart={() => setComposing(true)}
          onCompositionEnd={(event) => {
            setComposing(false);
            setSearchInput(event.currentTarget.value);
          }}
          placeholder={
            isTv
              ? t("media.sidebar.searchTvPlaceholder")
              : t("media.sidebar.searchMoviePlaceholder")
          }
          suffix={
            searchInput && (
              <button
                type="button"
                className="cursor-pointer text-fg-muted hover:text-fg-primary"
                aria-label={t("media.sidebar.clearSearch")}
                onClick={() => {
                  setSearchInput("");
                  if (search) {
                    setSearch("");
                    resetAll();
                  }
                }}
              >
                <X />
              </button>
            )
          }
        />
        <p className="mt-2 text-xs text-fg-muted" role="status">
          {paginatedQuery.isFetching || searchInput.trim() !== search
            ? t("media.search.searching")
            : paginatedQuery.isError
              ? t("media.sidebar.loadFailed")
              : t("media.sidebar.resultCount", { total })}
        </p>
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
        {paginatedQuery.isError && (
          <div
            role="alert"
            className="mb-3 flex items-center justify-between gap-3 text-sm text-state-danger-text"
          >
            <span>{t("media.sidebar.loadFailed")}</span>
            <button
              type="button"
              className="cursor-pointer text-accent-text"
              onClick={() => void paginatedQuery.refetch()}
            >
              {t("media.sidebar.retry")}
            </button>
          </div>
        )}
        {isLoading && items.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <Spin />
          </div>
        ) : items.length === 0 && !paginatedQuery.isError ? (
          <Empty
            className="flex h-full items-center justify-center"
            description={
              search
                ? t("media.sidebar.emptySearch")
                : activeFilterCount > 0
                  ? t("media.sidebar.emptyFiltered")
                  : t("media.sidebar.emptySyncFirst")
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
                <motion.div
                  key={item.id}
                  layout={active}
                  transition={LAYOUT_SPRING}
                  onClickCapture={(event) => rememberCard(event.currentTarget)}
                >
                  <MediaCard
                    item={item}
                    landscape={isLandscape}
                    onClick={() => handleItemClick(item)}
                  />
                </motion.div>
              ))}
            </div>

            <div ref={visibleSentinelRef} className="h-px" />
            <div className="mt-2 flex justify-center py-3">
              {paginatedQuery.isFetching && <Spin />}
              {!hasMore &&
                total > 0 &&
                !paginatedQuery.isFetching &&
                !paginatedQuery.isError && (
                  <p className="text-xs text-fg-muted">
                    {t("media.sidebar.allLoaded", { total })}
                  </p>
                )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
