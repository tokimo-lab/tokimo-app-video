import { posterThumbUrl } from "@tokimo/sdk";
import { ArrowLeftOutlined, Button } from "@tokimo/ui";
import type { ReactNode } from "react";
import type { CollectionOutput, GenreOutput } from "../shell-shim/types";
import {
  formatRuntime,
  MediaInfoBlock,
  MediaPoster,
  MediaTagsRow,
  SectionTitle,
} from "./media-detail-shared";

// ── Outer page shell ──────────────────────────────────────────────────────────

export function MediaDetailLayout({
  onBack,
  title,
  posterPath,
  posterFallbackEmoji,
  posterLandscape,
  posterOverlay,
  headerContent,
  children,
}: {
  onBack: () => void;
  title: string;
  posterPath?: string | null;
  posterFallbackEmoji: string;
  posterLandscape?: boolean;
  /** Absolutely-positioned overlay rendered on top of the poster (e.g. play button) */
  posterOverlay?: ReactNode;
  /** Right column: title, ratings, tags, actions */
  headerContent: ReactNode;
  /** Body below the header */
  children: ReactNode;
}) {
  return (
    <div className="-mx-3 -mt-3 -mb-3 relative min-h-full lg:-mx-4 lg:-mt-4 lg:-mb-4">
      <div className="relative z-10 px-6 pt-6 pb-6">
        <div className="mb-6">
          <Button icon={<ArrowLeftOutlined />} onClick={onBack}>
            返回
          </Button>
        </div>
        <div className="flex items-start gap-6">
          <div className="relative hidden flex-shrink-0 md:block">
            <MediaPoster
              posterPath={posterPath}
              title={title}
              fallbackEmoji={posterFallbackEmoji}
              landscape={posterLandscape}
            />
            {posterOverlay}
          </div>
          <div className="min-w-0 flex-1">{headerContent}</div>
        </div>
      </div>
      <div className="relative z-10 px-6 pt-6 pb-6">{children}</div>
    </div>
  );
}

// ── Info panel (right column of the header) ───────────────────────────────────

export function MediaDetailMeta({
  title,
  originalTitle,
  tagline,
  favoriteSlot,
  yearDisplay,
  runtime,
  contentRating,
  tmdbRating,
  imdbRating,
  doubanRating,
  extraBadges,
  genres,
  tmdbId,
  imdbId,
  tvdbId,
  mediaType,
  directors = [],
  writers = [],
  date,
  dateLabel,
  countries,
  children,
}: {
  title: string;
  originalTitle?: string | null;
  tagline?: string | null;
  favoriteSlot: ReactNode;
  /** Pre-computed year/release string to render (avoids page-specific logic leaking in) */
  yearDisplay?: string | number | null;
  runtime?: number | null;
  contentRating?: string | null;
  tmdbRating?: number | null;
  imdbRating?: number | null;
  doubanRating?: number | null;
  /** Extra badges rendered inside the metadata row (e.g. season count, status, scrapedAt) */
  extraBadges?: ReactNode;
  genres?: GenreOutput[] | null;
  tmdbId?: string | null;
  imdbId?: string | null;
  tvdbId?: string | null;
  mediaType: "movie" | "tv";
  directors?: string[];
  writers?: string[];
  date?: string | null;
  dateLabel: string;
  countries?: string[] | null;
  /** Extra content rendered below the info block (e.g. play button, uploader info) */
  children?: ReactNode;
}) {
  const hasSubtitle = !!((originalTitle && originalTitle !== title) || tagline);

  return (
    <>
      <div className="flex items-center gap-2">
        <h1 className="text-3xl font-bold leading-tight">{title}</h1>
        {favoriteSlot}
      </div>

      {hasSubtitle && (
        <p className="mt-0.5 truncate text-sm text-fg-muted">
          {originalTitle && originalTitle !== title ? originalTitle : null}
          {originalTitle && originalTitle !== title && tagline && (
            <span className="mx-1">·</span>
          )}
          {tagline ? <span className="italic">{tagline}</span> : null}
        </p>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
        {yearDisplay != null && (
          <span className="text-fg-secondary">{yearDisplay}</span>
        )}
        {runtime != null && (
          <span className="text-fg-secondary">
            {yearDisplay != null ? "· " : ""}
            {formatRuntime(runtime)}
          </span>
        )}
        {contentRating && (
          <span className="rounded border border-border-base px-1.5 py-0.5 text-xs text-fg-secondary">
            {contentRating}
          </span>
        )}
        {tmdbRating != null && (
          <span className="rounded bg-yellow-500/20 px-2 py-0.5 text-xs font-semibold text-yellow-600 dark:text-yellow-400">
            TMDB ★ {tmdbRating.toFixed(1)}
          </span>
        )}
        {imdbRating != null && (
          <span className="rounded bg-amber-500/20 px-2 py-0.5 text-xs font-semibold text-amber-600 dark:text-amber-400">
            IMDb ★ {imdbRating.toFixed(1)}
          </span>
        )}
        {doubanRating != null && (
          <span className="rounded bg-green-500/20 px-2 py-0.5 text-xs font-semibold text-green-600 dark:text-green-400">
            豆瓣 ★ {doubanRating.toFixed(1)}
          </span>
        )}
        {extraBadges}
        <MediaTagsRow
          genres={genres}
          tmdbId={tmdbId}
          imdbId={imdbId}
          tvdbId={tvdbId}
          mediaType={mediaType}
        />
      </div>

      <MediaInfoBlock
        directors={directors}
        writers={writers}
        date={date}
        dateLabel={dateLabel}
        countries={countries}
      />

      {children}
    </>
  );
}

// ── Reusable body sections ────────────────────────────────────────────────────

export function OverviewSection({ overview }: { overview?: string | null }) {
  if (!overview) return null;
  return (
    <div className="mb-8">
      <SectionTitle>简介</SectionTitle>
      <p className="text-sm leading-relaxed text-fg-secondary">{overview}</p>
    </div>
  );
}

export function CollectionsSection({
  collections,
}: {
  collections?: CollectionOutput[] | null;
}) {
  if (!collections?.length) return null;
  return (
    <section className="mb-8">
      <SectionTitle>所属合集</SectionTitle>
      <div className="flex gap-3 overflow-x-auto pb-2">
        {collections.map((col) => (
          <div
            key={col.id}
            className="flex w-[200px] flex-shrink-0 items-center gap-2.5 rounded-lg border border-border-base p-2"
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
  );
}
