import type { KeyboardEvent, PointerEvent } from "react";
import { useCallback, useRef, useState } from "react";
import type { ChapterOutput } from "@/types";

interface PlayerSeekBarProps {
  currentTime: number;
  duration: number;
  bufferedRanges?: { start: number; end: number }[];
  onSeek: (time: number) => void;
  chapters?: ChapterOutput[];
  className?: string;
  ariaLabel?: string;
}

const DEFAULT_CLASS_NAME =
  "group/seek flex h-5 cursor-pointer items-center focus:outline-none";

function formatTime(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const secs = safeSeconds % 60;

  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

export function PlayerSeekBar({
  currentTime,
  duration,
  bufferedRanges = [],
  onSeek,
  chapters = [],
  className = DEFAULT_CLASS_NAME,
  ariaLabel = "播放进度",
}: PlayerSeekBarProps) {
  const barRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef(false);
  const dragPctRef = useRef<number | null>(null);
  const onSeekRef = useRef(onSeek);
  onSeekRef.current = onSeek;
  const durationRef = useRef(duration);
  durationRef.current = duration;
  const [dragPct, setDragPct] = useState<number | null>(null);
  const [hoverPct, setHoverPct] = useState<number | null>(null);

  const pctFromClientX = useCallback((clientX: number) => {
    const rect = barRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return 0;
    return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  }, []);

  const handlePointerDown = useCallback(
    (e: PointerEvent<HTMLDivElement>) => {
      e.stopPropagation();
      e.preventDefault();
      draggingRef.current = true;
      const pct = pctFromClientX(e.clientX);
      dragPctRef.current = pct;
      setDragPct(pct);
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [pctFromClientX],
  );

  const handlePointerMove = useCallback(
    (e: PointerEvent<HTMLDivElement>) => {
      if (!draggingRef.current) {
        setHoverPct(pctFromClientX(e.clientX));
        return;
      }
      const pct = pctFromClientX(e.clientX);
      dragPctRef.current = pct;
      setDragPct(pct);
    },
    [pctFromClientX],
  );

  const handlePointerUp = useCallback((e: PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    e.currentTarget.releasePointerCapture(e.pointerId);
    draggingRef.current = false;
    if (dragPctRef.current !== null) {
      onSeekRef.current(dragPctRef.current * durationRef.current);
    }
    dragPctRef.current = null;
    setDragPct(null);
  }, []);

  const handlePointerLeave = useCallback(() => {
    if (!draggingRef.current) {
      setHoverPct(null);
    }
  }, []);

  const playedPct = duration > 0 ? currentTime / duration : 0;
  const displayPct = dragPct ?? playedPct;

  // Like Jellyfin: find the first buffered range that extends beyond playback position
  let bufferStartPct = 0;
  let bufferWidthPct = 0;
  if (duration > 0) {
    for (const range of bufferedRanges) {
      const endPct = range.end / duration;
      if (endPct > playedPct) {
        bufferStartPct = Math.max(0, range.start / duration) * 100;
        bufferWidthPct =
          Math.min(Math.max(endPct - range.start / duration, 0), 1) * 100;
        break;
      }
    }
  }
  const tooltipPct = dragPct ?? hoverPct;

  // Find current chapter name for the hovered / dragged position
  const tooltipChapterName =
    tooltipPct !== null && chapters.length > 0 && duration > 0
      ? (() => {
          const time = tooltipPct * duration;
          let found: ChapterOutput | null = null;
          for (const ch of chapters) {
            if (ch.startTime <= time) found = ch;
            else break;
          }
          return found?.title ?? null;
        })()
      : null;

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      onSeek(Math.max(0, currentTime - 5));
    }

    if (event.key === "ArrowRight") {
      event.preventDefault();
      onSeek(Math.min(durationRef.current, currentTime + 5));
    }
  };

  return (
    <div
      ref={barRef}
      role="slider"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(displayPct * 100)}
      className={className}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerLeave={handlePointerLeave}
      onKeyDown={handleKeyDown}
    >
      <div className="relative h-1 w-full overflow-visible rounded-full transition-[height] duration-100 group-hover/seek:h-1.5 group-focus-within/seek:h-1.5">
        {tooltipPct !== null && (
          <div
            className="pointer-events-none absolute bottom-full z-10 mb-2 -translate-x-1/2 rounded bg-black/65 px-2 py-1 text-center text-[10px] font-medium text-white shadow-lg backdrop-blur-xl ring-1 ring-white/10"
            style={{ left: `${tooltipPct * 100}%` }}
          >
            {tooltipChapterName && (
              <div className="mb-0.5 max-w-[160px] truncate text-white/80">
                {tooltipChapterName}
              </div>
            )}
            {formatTime(tooltipPct * durationRef.current)}
          </div>
        )}
        {/* Background track */}
        <div className="absolute inset-0 rounded-full bg-white/30" />
        {/* Buffered track */}
        <div
          className="absolute inset-y-0 rounded-full bg-white/40"
          style={{ left: `${bufferStartPct}%`, width: `${bufferWidthPct}%` }}
        />
        {/* Played track (accent color) */}
        <div
          className="absolute inset-y-0 left-0 rounded-full bg-[var(--accent)]"
          style={{ width: `${displayPct * 100}%` }}
        />
        {/* Chapter markers */}
        {chapters.length > 0 &&
          duration > 0 &&
          chapters.map((chapter) => {
            const pct = (chapter.startTime / duration) * 100;
            if (pct <= 0 || pct >= 100) return null;
            const label = chapter.title || `第 ${chapter.index} 章`;
            return (
              <button
                key={chapter.id}
                type="button"
                aria-label={`跳转到 ${label}`}
                title={`${label}\n${formatTime(chapter.startTime)}`}
                className="group/ch absolute top-1/2 z-10 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 cursor-pointer rounded-full border-[1.5px] border-white/70 bg-white/30 shadow transition-all duration-100 hover:scale-150 hover:border-white hover:bg-white"
                style={{ left: `${pct}%` }}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  onSeek(chapter.startTime);
                }}
              />
            );
          })}
        {/* Draggable thumb */}
        <div
          className={`absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow transition-opacity ${
            dragPct !== null
              ? "opacity-100"
              : "opacity-0 group-hover/seek:opacity-100 group-focus-within/seek:opacity-100"
          }`}
          style={{ left: `${displayPct * 100}%` }}
        />
      </div>
    </div>
  );
}
