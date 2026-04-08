/**
 * EpisodeListMenu — TV series episode picker overlay for the video player.
 *
 * Shows a "EP XX/XX" button in the player toolbar. Clicking opens a frosted-glass
 * panel with scrollable episode list (thumbnails + titles). Clicking an episode
 * switches playback. Auto-scrolls to the currently playing episode on open.
 */
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "@/generated/rust-api";
import { posterThumbUrl } from "@/lib/thumb";
import { usePlayer } from "@/system";
import type { EpisodeOutput, MediaFileOutput } from "@/types";
import {
  PlayerControlTooltip,
  useDismissOnOutsidePointerDown,
} from "./player-controls-shared";

export const EpisodeListMenu = memo(function EpisodeListMenu() {
  const { item, play } = usePlayer();
  const [open, setOpen] = useState(false);
  const portalRef = useRef<HTMLDivElement | null>(null);
  const containerRef = useDismissOnOutsidePointerDown(
    open,
    () => setOpen(false),
    [],
    [portalRef],
  );

  const tvShowId = item?.tvShowId;
  const episodeId = item?.episodeId;

  const { data: tvShow } = api.video.getTvShowDetail.useQuery(
    { id: tvShowId! },
    { enabled: !!tvShowId },
  );

  // Flatten all episodes across seasons
  const allEpisodes =
    tvShow?.seasons?.flatMap((s) =>
      (s.episodes ?? []).map((ep) => ({
        ...ep,
        seasonNumber: s.seasonNumber,
      })),
    ) ?? [];

  const currentIdx = allEpisodes.findIndex((ep) => ep.id === episodeId);
  const total = allEpisodes.length;

  // Don't render if not a TV episode or no episodes loaded
  if (!tvShowId || !episodeId || total === 0) return null;

  const displayIdx = currentIdx >= 0 ? currentIdx + 1 : 1;

  return (
    <div ref={containerRef} className="relative">
      <PlayerControlTooltip title="剧集列表">
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setOpen((o) => !o);
          }}
          className="flex h-8 cursor-pointer items-center gap-1 rounded px-2 text-xs font-medium text-white/80 hover:bg-white/10 hover:text-white"
        >
          <svg
            className="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <rect x="3" y="3" width="7" height="7" rx="1" />
            <rect x="14" y="3" width="7" height="7" rx="1" />
            <rect x="3" y="14" width="7" height="7" rx="1" />
            <rect x="14" y="14" width="7" height="7" rx="1" />
          </svg>
          <span className="text-[11px] tabular-nums">
            {displayIdx}/{total}
          </span>
        </button>
      </PlayerControlTooltip>
      {open &&
        createPortal(
          <EpisodeListPanel
            ref={portalRef}
            episodes={allEpisodes}
            currentEpisodeId={episodeId}
            tvShow={tvShow!}
            onSelect={(ep) => {
              const file = ep.files?.[0];
              if (!file) return;
              play(file as MediaFileOutput, {
                title: ep.title ?? `第 ${ep.episodeNumber} 集`,
                posterPath: tvShow?.posterPath,
                tvShowId: tvShowId,
                episodeId: ep.id,
                imdbId: tvShow?.imdbId,
                tmdbId: tvShow?.tmdbId,
              });
              setOpen(false);
            }}
            onClose={() => setOpen(false)}
          />,
          document.body,
        )}
    </div>
  );
});

EpisodeListMenu.displayName = "EpisodeListMenu";

// ── Episode list panel (frosted glass overlay) ────────────────────────────────

interface EpisodeWithSeason extends EpisodeOutput {
  seasonNumber: number;
}

const EpisodeListPanel = memo(function EpisodeListPanel({
  ref,
  episodes,
  currentEpisodeId,
  tvShow,
  onSelect,
  onClose,
}: {
  ref: React.Ref<HTMLDivElement>;
  episodes: EpisodeWithSeason[];
  currentEpisodeId: string;
  tvShow: { title: string; seasons?: { seasonNumber: number }[] };
  onSelect: (ep: EpisodeWithSeason) => void;
  onClose: () => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLButtonElement>(null);
  const hasMultipleSeasons = (tvShow.seasons?.length ?? 0) > 1;

  // Auto-scroll to current episode on mount
  useEffect(() => {
    const timer = setTimeout(() => {
      activeRef.current?.scrollIntoView({
        block: "center",
        behavior: "instant",
      });
    }, 50);
    return () => clearTimeout(timer);
  }, []);

  // Close on Escape
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  // Group by season for multi-season shows
  const seasonGroups = hasMultipleSeasons
    ? groupBySeason(episodes)
    : [[0, episodes] as const];

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: player overlay backdrop
    // biome-ignore lint/a11y/useKeyWithClickEvents: player overlay backdrop
    <div
      ref={ref}
      className="player-popup-in fixed inset-0 z-[99999] flex items-center justify-center"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[80vh] w-full max-w-[28rem] flex-col overflow-hidden rounded-2xl bg-black/60 shadow-2xl ring-1 ring-white/15 backdrop-blur-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-white/10 px-5 py-3">
          <h3 className="text-sm font-medium text-white">剧集列表</h3>
          <button
            type="button"
            className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-full text-white/60 hover:bg-white/10 hover:text-white"
            onClick={onClose}
          >
            <svg
              className="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2.5}
            >
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Episode list */}
        <div ref={listRef} className="overflow-y-auto p-2">
          {seasonGroups.map(([seasonNum, eps]) => (
            <div key={seasonNum}>
              {hasMultipleSeasons && (
                <div className="px-3 py-2 text-xs font-medium text-white/50">
                  第 {seasonNum} 季
                </div>
              )}
              {eps.map((ep) => {
                const isCurrent = ep.id === currentEpisodeId;
                const hasFile = (ep.files?.length ?? 0) > 0;
                return (
                  <EpisodeItem
                    key={ep.id}
                    ref={isCurrent ? activeRef : undefined}
                    episode={ep}
                    isCurrent={isCurrent}
                    hasFile={hasFile}
                    onClick={() => onSelect(ep)}
                  />
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
});

EpisodeListPanel.displayName = "EpisodeListPanel";

// ── Single episode item ───────────────────────────────────────────────────────

const EpisodeItem = memo(function EpisodeItem({
  ref,
  episode,
  isCurrent,
  hasFile,
  onClick,
}: {
  ref?: React.Ref<HTMLButtonElement>;
  episode: EpisodeWithSeason;
  isCurrent: boolean;
  hasFile: boolean;
  onClick: () => void;
}) {
  const thumb = posterThumbUrl(episode.stillPath, 160);
  const handleClick = useCallback(() => {
    if (hasFile) onClick();
  }, [hasFile, onClick]);

  return (
    <button
      ref={ref}
      type="button"
      disabled={!hasFile}
      onClick={handleClick}
      className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors ${
        isCurrent
          ? "bg-white/15 text-white"
          : hasFile
            ? "cursor-pointer text-white/80 hover:bg-white/10 hover:text-white"
            : "cursor-default text-white/30"
      }`}
    >
      {/* Thumbnail */}
      <div className="relative h-[48px] w-[85px] flex-shrink-0 overflow-hidden rounded bg-white/5">
        {thumb ? (
          <img
            src={thumb}
            alt=""
            className="h-full w-full object-cover"
            loading="lazy"
          />
        ) : (
          <div className="flex h-full w-full items-center justify-center text-white/20">
            <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z" />
            </svg>
          </div>
        )}
        {isCurrent && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/40">
            <div className="flex items-center gap-0.5">
              <span className="inline-block h-3 w-0.5 animate-pulse rounded-full bg-white" />
              <span
                className="inline-block h-4 w-0.5 animate-pulse rounded-full bg-white"
                style={{ animationDelay: "0.15s" }}
              />
              <span
                className="inline-block h-2.5 w-0.5 animate-pulse rounded-full bg-white"
                style={{ animationDelay: "0.3s" }}
              />
            </div>
          </div>
        )}
      </div>

      {/* Title */}
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs font-medium">
          第 {episode.episodeNumber} 集
          {episode.title ? ` · ${episode.title}` : ""}
        </div>
        {episode.runtime != null && episode.runtime > 0 && (
          <div className="mt-0.5 text-[10px] text-white/40">
            {episode.runtime} 分钟
          </div>
        )}
      </div>

      {/* Playing indicator */}
      {isCurrent && (
        <span className="flex-shrink-0 text-[10px] font-medium text-[var(--accent)]">
          播放中
        </span>
      )}
    </button>
  );
});

EpisodeItem.displayName = "EpisodeItem";

// ── Helpers ───────────────────────────────────────────────────────────────────

function groupBySeason(
  episodes: EpisodeWithSeason[],
): [number, EpisodeWithSeason[]][] {
  const map = new Map<number, EpisodeWithSeason[]>();
  for (const ep of episodes) {
    const group = map.get(ep.seasonNumber);
    if (group) {
      group.push(ep);
    } else {
      map.set(ep.seasonNumber, [ep]);
    }
  }
  return [...map.entries()].sort(([a], [b]) => a - b);
}
