import { Empty, PillTabBar, PosterCard, Spin, Tag } from "@tokiomo/components";
import { getGenreName } from "@tokiomo/types";
import { ArrowDownUp, Clock, LayoutGrid } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { VideoOutput } from "@/generated/rust-api";
import { api } from "@/generated/rust-api";
import { posterThumbUrl } from "@/lib/thumb";
import { useLang, useWindowNav } from "@/system";
import type { TvShowOutput, VideoItemOutput } from "@/types";

const MIN_CARD_WIDTH = 150;
const CARD_GAP = 12;
const CARD_TITLE_HEIGHT = 52;

type MediaItem = (VideoItemOutput | TvShowOutput) & {
  posterPath?: string | null;
};

const POSTER_BADGE_CLASS =
  "absolute right-0 inline-flex items-center gap-1 rounded-l-md rounded-r-none border border-r-0 border-white/12 bg-[var(--sidebar-bg)] px-2 py-1 text-[10px] font-medium shadow-sm backdrop-blur-md";

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
        className="truncate text-sm font-medium text-fg-primary"
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

const SORT_OPTIONS = [
  { label: "最近添加", value: "addedAt" },
  { label: "标题 A-Z", value: "title_asc" },
  { label: "标题 Z-A", value: "title_desc" },
  { label: "年份 最新", value: "year_desc" },
  { label: "年份 最旧", value: "year_asc" },
  { label: "评分 最高", value: "rating" },
] as const;

type SortValue = (typeof SORT_OPTIONS)[number]["value"];
type MediaTabKey = "all" | "recent";

function parseSortValue(v: SortValue) {
  if (v === "title_asc") return { sortBy: "title", sortDir: "asc" };
  if (v === "title_desc") return { sortBy: "title", sortDir: "desc" };
  if (v === "year_desc") return { sortBy: "year", sortDir: "desc" };
  if (v === "year_asc") return { sortBy: "year", sortDir: "asc" };
  if (v === "rating") return { sortBy: "rating", sortDir: "desc" };
  return { sortBy: "addedAt", sortDir: "desc" };
}

export default function VideoContent({ category }: { category: VideoOutput }) {
  const { navigate } = useWindowNav();
  const { lang } = useLang();
  const id = category.id;
  const libType = category.type;
  const isTv = libType === "tv" || libType === "anime";
  const isLandscape = libType === "online_video";

  const [tab, setTabRaw] = useState<MediaTabKey>("all");
  const [page, setPage] = useState(1);
  const [allItems, setAllItems] = useState<MediaItem[]>([]);
  const [sortValue, setSortValue] = useState<SortValue>("addedAt");
  const [genreId, setGenreId] = useState<string | undefined>(undefined);
  const lastAppendedPageRef = useRef(0);
  const isLoadingMoreRef = useRef(false);

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

  const resetPagination = useCallback(() => {
    lastAppendedPageRef.current = 0;
    setAllItems([]);
    setPage(1);
  }, []);

  // Reset when switching category
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally reset on id change
  useEffect(() => {
    lastAppendedPageRef.current = 0;
    setAllItems([]);
    setPage(1);
    setTabRaw("all");
    setSortValue("addedAt");
    setGenreId(undefined);
  }, [id]);

  const sortParams = parseSortValue(sortValue);

  const genresQuery = api.video.listGenres.useQuery({ id }, { enabled: !!id });
  const genres = genresQuery.data ?? [];

  const recentQuery = api.video.getRecentlyAdded.useQuery(
    { id, limit: 50 },
    { enabled: !!id },
  );
  const recentItems = (recentQuery.data ?? []) as unknown as MediaItem[];

  const moviesQuery = api.video.listMovies.useQuery(
    { id, page, pageSize, ...sortParams, genreId },
    { enabled: !!id && !isTv && pageSize > 0 },
  );

  const tvQuery = api.video.listTvShows.useQuery(
    { id, page, pageSize, ...sortParams, genreId },
    { enabled: !!id && isTv && pageSize > 0 },
  );

  const paginatedQuery = isTv ? tvQuery : moviesQuery;
  const paginatedTotal = paginatedQuery.data?.total ?? 0;
  const hasMore = allItems.length < paginatedTotal;

  const displayItems = tab === "recent" ? recentItems : allItems;
  const displayTotal = tab === "recent" ? recentItems.length : paginatedTotal;
  const isLoading =
    tab === "recent" ? recentQuery.isLoading : paginatedQuery.isLoading;

  const setTab = useCallback(
    (t: MediaTabKey) => {
      if (t === tab) return;
      setTabRaw(t);
      if (t === "all") resetPagination();
    },
    [tab, resetPagination],
  );

  useEffect(() => {
    const data = paginatedQuery.data;
    if (!data || paginatedQuery.isFetching) return;
    isLoadingMoreRef.current = false;
    const currentPage = data.page;
    if (currentPage === lastAppendedPageRef.current) return;
    lastAppendedPageRef.current = currentPage;
    if (currentPage === 1) {
      setAllItems(data.items as MediaItem[]);
    } else {
      setAllItems((prev) => [...prev, ...(data.items as MediaItem[])]);
    }
  }, [paginatedQuery.data, paginatedQuery.isFetching]);

  // Infinite scroll
  useEffect(() => {
    let container: HTMLElement | null =
      gridWrapperRef.current?.parentElement ?? null;
    while (container) {
      const ov = getComputedStyle(container).overflowY;
      if (ov === "auto" || ov === "scroll") break;
      container = container.parentElement;
    }
    if (!container) return;

    const check = () => {
      if (
        tab !== "all" ||
        isLoadingMoreRef.current ||
        !hasMore ||
        paginatedQuery.isFetching
      )
        return;
      const { scrollTop, scrollHeight, clientHeight } = container;
      if (scrollTop + clientHeight >= scrollHeight - 600) {
        isLoadingMoreRef.current = true;
        setPage((p) => p + 1);
      }
    };

    container.addEventListener("scroll", check, { passive: true });
    check();
    return () => container.removeEventListener("scroll", check);
  }, [tab, hasMore, paginatedQuery.isFetching]);

  const handleItemClick = useCallback(
    (item: MediaItem) => {
      if (isTv) {
        navigate(`/tv/${item.id}`, item.title ?? "TV Show");
      } else {
        navigate(`/movies/${item.id}`, item.title ?? "Movie");
      }
    },
    [isTv, navigate],
  );

  const handleSortChange = (v: SortValue) => {
    setSortValue(v);
    resetPagination();
  };

  const handleGenreChange = (gid: string | undefined) => {
    setGenreId(gid);
    resetPagination();
  };

  const MEDIA_TABS: { key: MediaTabKey; label: string; icon: typeof Clock }[] =
    [
      { key: "recent", label: "最近添加", icon: Clock },
      { key: "all", label: "全部", icon: LayoutGrid },
    ];

  const genreOptions = [
    { label: "全部", value: "" },
    ...genres.map((g) => ({
      label: getGenreName(g.tmdbGenreId, lang) || g.name,
      value: g.id,
    })),
  ];

  return (
    <div className="p-4">
      <PillTabBar
        tabs={MEDIA_TABS}
        activeTab={tab}
        onTabChange={setTab}
        sort={
          tab === "all"
            ? {
                options: SORT_OPTIONS,
                value: sortValue,
                onChange: (v) => handleSortChange(v as SortValue),
                activeIcon: <ArrowDownUp className="h-3.5 w-3.5" />,
              }
            : undefined
        }
        filters={
          tab === "all" && genres.length > 0
            ? [
                {
                  label: "类型",
                  options: genreOptions,
                  value: genreId ?? "",
                  onChange: (v) => handleGenreChange(v || undefined),
                },
              ]
            : undefined
        }
        trailing={displayTotal > 0 ? <Tag>{displayTotal}</Tag> : undefined}
      />

      <div ref={gridWrapperRef} className="mt-3">
        {isLoading && displayItems.length === 0 ? (
          <div className="flex h-64 items-center justify-center">
            <Spin />
          </div>
        ) : displayItems.length === 0 ? (
          <Empty
            description={
              tab === "recent"
                ? "暂无最近添加的资源"
                : genreId
                  ? "该类型暂无资源"
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
              {displayItems.map((item) => (
                <MediaCard
                  key={item.id}
                  item={item}
                  landscape={isLandscape}
                  onClick={() => handleItemClick(item)}
                />
              ))}
            </div>

            {tab === "all" && (
              <div className="mt-2 flex justify-center py-3">
                {paginatedQuery.isFetching && <Spin />}
                {!hasMore &&
                  paginatedTotal > 0 &&
                  !paginatedQuery.isFetching && (
                    <p className="text-xs text-fg-muted">
                      已加载全部 {paginatedTotal} 个
                    </p>
                  )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
