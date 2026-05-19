/**
 * EpisodeListMenu — TV series episode picker overlay for the video player.
 *
 * Shows a "EP XX/XX" button in the player toolbar. Clicking opens a frosted-glass
 * panel anchored to the bottom-right (above the toolbar). Clicking an episode
 * switches playback. Auto-scrolls to the currently playing episode on open.
 *
 * Also provides prev/next episode navigation buttons and auto-play-next on ended.
 */

import { posterThumbUrl } from "@tokimo/sdk";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { api, type EpisodeOutput, type MediaFileOutput } from "../api";
import { usePlayer, useVideoUiState } from "../hooks/shell-stubs";
import {
  PlayerControlTooltip,
  useDismissOnOutsidePointerDown,
  useDropdownPortalPos,
} from "./player-controls-shared";

// ── Types ─────────────────────────────────────────────────────────────────────

interface EpisodeWithSeason extends EpisodeOutput {
  seasonNumber: number;
}

// ── Main button + panel controller ────────────────────────────────────────────

export const EpisodeListMenu = memo(function EpisodeListMenu() {
  const { t } = useTranslation();
  const { item, play } = usePlayer();
  const { onEndedRef } = useVideoUiState();
  const [open, setOpen] = useState(false);
  const portalRef = useRef<HTMLDivElement | null>(null);
  const dismissRef = useDismissOnOutsidePointerDown(
    open,
    () => setOpen(false),
    [],
    [portalRef],
  );
  const portalPos = useDropdownPortalPos(dismissRef, open);

  const tvShowId = item?.tvShowId;
  const episodeId = item?.episodeId;

  const { data: tvShow } = api.video.getTvShowDetail.useQuery(
    { id: tvShowId! },
    { enabled: !!tvShowId },
  );

  const allEpisodes = useMemo(
    () =>
      tvShow?.seasons?.flatMap((s) =>
        (s.episodes ?? []).map((ep) => ({
          ...ep,
          seasonNumber: s.seasonNumber,
        })),
      ) ?? [],
    [tvShow],
  );

  const currentIdx = allEpisodes.findIndex((ep) => ep.id === episodeId);
  const total = allEpisodes.length;

  const playEpisode = useCallback(
    (ep: EpisodeWithSeason) => {
      const file = ep.files?.[0];
      if (!file) return;
      play(file as MediaFileOutput, {
        title:
          ep.title ??
          t("media.detail.episodeNumber", { number: ep.episodeNumber }),
        posterPath: tvShow?.posterPath,
        tvShowId,
        episodeId: ep.id,
        imdbId: tvShow?.imdbId,
        tmdbId: tvShow?.tmdbId,
      });
    },
    [play, tvShow, tvShowId, t],
  );

  const playNext = useCallback(() => {
    if (currentIdx < 0 || currentIdx >= total - 1) return;
    const next = allEpisodes[currentIdx + 1];
    if (next) playEpisode(next);
  }, [currentIdx, total, allEpisodes, playEpisode]);

  const playPrev = useCallback(() => {
    if (currentIdx <= 0) return;
    const prev = allEpisodes[currentIdx - 1];
    if (prev) playEpisode(prev);
  }, [currentIdx, allEpisodes, playEpisode]);

  // Auto-play next episode when current one ends
  useEffect(() => {
    onEndedRef.current = () => {
      playNext();
    };
    return () => {
      onEndedRef.current = null;
    };
  }, [onEndedRef, playNext]);

  if (!tvShowId || !episodeId || total === 0) return null;

  const displayIdx = currentIdx >= 0 ? currentIdx + 1 : 1;
  const hasPrev = currentIdx > 0;
  const hasNext = currentIdx >= 0 && currentIdx < total - 1;

  return (
    <div ref={dismissRef} className="relative flex items-center gap-0.5">
      {/* Prev episode */}
      <PlayerControlTooltip title={t("media.viewer.previousEpisode")}>
        <button
          type="button"
          disabled={!hasPrev}
          onClick={() => playPrev()}
          className={`flex h-8 w-8 items-center justify-center rounded ${
            hasPrev
              ? "cursor-pointer text-white/80 hover:bg-white/10 hover:text-white"
              : "cursor-default text-white/25"
          }`}
          aria-label={t("media.viewer.previousEpisode")}
        >
          <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 6h2v12H6zm3.5 6 8.5 6V6z" />
          </svg>
        </button>
      </PlayerControlTooltip>

      {/* Episode list toggle */}
      <PlayerControlTooltip title={t("media.viewer.episodeList")}>
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

      {/* Next episode */}
      <PlayerControlTooltip title={t("media.viewer.nextEpisode")}>
        <button
          type="button"
          disabled={!hasNext}
          onClick={() => playNext()}
          className={`flex h-8 w-8 items-center justify-center rounded ${
            hasNext
              ? "cursor-pointer text-white/80 hover:bg-white/10 hover:text-white"
              : "cursor-default text-white/25"
          }`}
          aria-label={t("media.viewer.nextEpisode")}
        >
          <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
          </svg>
        </button>
      </PlayerControlTooltip>

      {/* Episode list panel — fixed, anchored bottom-right above toolbar */}
      {open &&
        portalPos &&
        createPortal(
          <EpisodeListPanel
            ref={portalRef}
            episodes={allEpisodes}
            currentEpisodeId={episodeId}
            tvShow={tvShow!}
            portalPos={portalPos}
            onSelect={(ep) => {
              playEpisode(ep);
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

// ── Episode list panel (frosted glass dropdown) ───────────────────────────────

const EpisodeListPanel = memo(function EpisodeListPanel({
  ref,
  episodes,
  currentEpisodeId,
  tvShow,
  portalPos,
  onSelect,
  onClose,
}: {
  ref: React.Ref<HTMLDivElement>;
  episodes: EpisodeWithSeason[];
  currentEpisodeId: string;
  tvShow: { title: string; seasons?: { seasonNumber: number }[] };
  portalPos: { right: number; bottom: number };
  onSelect: (ep: EpisodeWithSeason) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const activeRef = useRef<HTMLButtonElement>(null);
  const hasMultipleSeasons = (tvShow.seasons?.length ?? 0) > 1;

  useEffect(() => {
    const timer = setTimeout(() => {
      activeRef.current?.scrollIntoView({
        block: "center",
        behavior: "instant",
      });
    }, 50);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose]);

  const seasonGroups = hasMultipleSeasons
    ? groupBySeason(episodes)
    : [[0, episodes] as const];

  return (
    <div
      ref={ref}
      className="player-popup-in fixed z-[99999] flex w-[22rem] flex-col overflow-hidden rounded-lg bg-black/65 shadow-2xl ring-1 ring-white/15 backdrop-blur-2xl"
      style={{
        right: portalPos.right,
        bottom: portalPos.bottom,
        maxHeight: "min(400px, 60vh)",
      }}
    >
      {/* Header */}
      <div className="flex flex-shrink-0 items-center justify-between border-b border-white/10 px-4 py-2.5">
        <h3 className="text-xs font-medium text-white/70">
          {t("media.viewer.episodeList")}
        </h3>
        <button
          type="button"
          className="flex h-5 w-5 cursor-pointer items-center justify-center rounded-full text-white/50 hover:bg-white/10 hover:text-white"
          onClick={onClose}
        >
          <svg
            className="h-3 w-3"
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
      <div className="overflow-y-auto p-1.5">
        {seasonGroups.map(([seasonNum, eps]) => (
          <div key={seasonNum}>
            {hasMultipleSeasons && (
              <div className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-white/40">
                {t("media.detail.seasonNumber", { number: seasonNum })}
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
  const { t } = useTranslation();
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
          {t("media.detail.episodeNumber", { number: episode.episodeNumber })}
          {episode.title ? ` · ${episode.title}` : ""}
        </div>
        {episode.runtime != null && episode.runtime > 0 && (
          <div className="mt-0.5 text-[10px] text-white/40">
            {t("media.detail.minutes", { count: episode.runtime })}
          </div>
        )}
      </div>

      {/* Playing indicator */}
      {isCurrent && (
        <span className="flex-shrink-0 text-[10px] font-medium text-[var(--accent)]">
          {t("media.viewer.playing")}
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
