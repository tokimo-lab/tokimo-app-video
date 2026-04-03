/**
 * CustomVideoControls — 自绘播放器控制层
 *
 * 播放进度、音量、字幕/音轨、标题栏等状态按更新频率拆分，
 * 避免 currentTime 每次跳动都拖着整条控制栏重渲染。
 * 包含：进度条、播放/暂停、±10s、音量、多音轨切换、字幕切换、缓冲动画。
 */
import { cn, Slider, useContextMenu } from "@tokiomo/components";
import { BarChart3 } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
  usePlayer,
  useVideoPlaybackSelector,
  useVideoTrackState,
  useVideoUiState,
  useVideoVolumeState,
} from "@/system";
import type { ChapterOutput } from "@/types";
import { SubtitleMenu } from "../subtitle/SubtitleMenu";
import { PlayerSeekBar } from "./PlayerSeekBar";
import {
  fmtTime,
  PlayerControlTooltip,
  renderAudioTrackLabel,
  renderAudioTriggerSummary,
  useDismissOnOutsidePointerDown,
  useDropdownPortalPos,
} from "./player-controls-shared";
import { VideoPlayerTitleBar } from "./VideoPlayerTitleBar";

// ── Volume control ────────────────────────────────────────────────────────────

const VolumeControl = memo(function VolumeControl() {
  const { volume, muted, changeVolume, mute, unmute } = useVideoVolumeState();
  const eff = muted ? 0 : volume;

  return (
    <div className="group/vol relative flex items-center">
      <PlayerControlTooltip title={muted ? "取消静音" : "静音"}>
        <button
          type="button"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded text-white/80 hover:bg-white/10 hover:text-white"
          aria-label={muted ? "取消静音" : "静音"}
          onClick={(e) => {
            e.stopPropagation();
            if (muted) {
              unmute();
            } else {
              mute();
            }
          }}
        >
          {muted || volume === 0 ? (
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M16.5 12A4.5 4.5 0 0 0 14 8v2.18l2.45 2.45c.05-.2.05-.42.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51A8.796 8.796 0 0 0 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06a8.99 8.99 0 0 0 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4 9.91 6.09 12 8.18V4z" />
            </svg>
          ) : volume < 0.5 ? (
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M18.5 12A4.5 4.5 0 0 0 16 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z" />
            </svg>
          ) : (
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0 0 14 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" />
            </svg>
          )}
        </button>
      </PlayerControlTooltip>
      {/* Volume slider — appears on hover via CSS group */}
      <Slider
        min={0}
        max={100}
        value={Math.round(eff * 100)}
        onChange={(v) => changeVolume(v / 100)}
        onClick={(e) => e.stopPropagation()}
        size="small"
        className="h-1 w-0 cursor-pointer opacity-0 transition-all duration-150 group-hover/vol:w-20 group-hover/vol:opacity-100"
        aria-label="音量"
      />
    </div>
  );
});

VolumeControl.displayName = "VolumeControl";

// ── Tone mapping toggle ───────────────────────────────────────────────────────

const ToneMappingToggle = memo(function ToneMappingToggle() {
  const { forceSDR, setForceSDR, item } = usePlayer();

  // 非 HDR 视频不需要色调映射，隐藏按钮
  const hdrType = item?.file.hdrType;
  const isHDR = !!hdrType && hdrType !== "sdr";
  if (!isHDR) return null;

  return (
    <PlayerControlTooltip
      title={forceSDR ? "HDR→SDR 色调映射已开启" : "HDR→SDR 色调映射已关闭"}
    >
      <button
        type="button"
        className={cn(
          "flex h-8 w-8 cursor-pointer items-center justify-center rounded hover:bg-white/10",
          forceSDR ? "text-blue-400" : "text-white/60",
        )}
        aria-label="Toggle HDR→SDR tone mapping"
        onClick={() => setForceSDR(!forceSDR)}
      >
        {/* HDR badge icon — filled when active */}
        <svg
          className="h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.6}
        >
          <rect x="2" y="5" width="20" height="14" rx="2" />
          <text
            x="12"
            y="14"
            textAnchor="middle"
            fill="currentColor"
            stroke="none"
            fontSize="7"
            fontWeight="bold"
            fontFamily="sans-serif"
          >
            {forceSDR ? "SDR" : "HDR"}
          </text>
        </svg>
      </button>
    </PlayerControlTooltip>
  );
});

ToneMappingToggle.displayName = "ToneMappingToggle";

// ── Audio track menu ──────────────────────────────────────────────────────────

const AudioTrackMenu = memo(function AudioTrackMenu() {
  const { i18n } = useTranslation();
  const { audioTracks, changeAudioTrack } = useVideoTrackState();
  const [open, setOpen] = useState(false);
  const didAutoSelect = useRef(false);
  const portalRef = useRef<HTMLDivElement | null>(null);
  const containerRef = useDismissOnOutsidePointerDown(
    open,
    () => setOpen(false),
    [],
    [portalRef],
  );
  const portalPos = useDropdownPortalPos(containerRef, open);
  const locale = i18n.resolvedLanguage ?? i18n.language ?? "zh-CN";

  useEffect(() => {
    if (audioTracks.length === 0) {
      didAutoSelect.current = false;
      return;
    }
    if (didAutoSelect.current) return;
    didAutoSelect.current = true;
    if (!audioTracks.some((t) => t.selected)) {
      changeAudioTrack(0);
    }
  }, [audioTracks, changeAudioTrack]);

  const selIdx = audioTracks.findIndex((t) => t.selected);
  const active = audioTracks[selIdx >= 0 ? selIdx : 0];
  const hasMultipleTracks = audioTracks.length > 1;
  const hasTracks = audioTracks.length > 0;

  return (
    <div ref={containerRef} className="relative">
      <PlayerControlTooltip title="音轨">
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setOpen((o) => !o);
          }}
          className="flex h-8 cursor-pointer items-center gap-1.5 rounded px-2 text-xs font-medium text-white/80 hover:bg-white/10 hover:text-white"
        >
          <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 3v10.55A4 4 0 1 0 14 17V7h4V3h-6z" />
          </svg>
          <span className="min-w-0 truncate text-[11px]">
            {active ? renderAudioTriggerSummary(active, locale) : "音轨"}
          </span>
        </button>
      </PlayerControlTooltip>
      {open &&
        portalPos &&
        createPortal(
          <div
            ref={portalRef}
            className="player-popup-in fixed z-[99999] min-w-[15rem] overflow-hidden rounded-lg bg-black/65 shadow-2xl ring-1 ring-white/15 backdrop-blur-2xl"
            style={{ right: portalPos.right, bottom: portalPos.bottom }}
          >
            {!hasTracks ? (
              <div className="px-3 py-2 text-xs text-white/60">
                未检测到音轨信息
              </div>
            ) : (
              audioTracks.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  disabled={!hasMultipleTracks}
                  onClick={(e) => {
                    if (!hasMultipleTracks) return;
                    e.stopPropagation();
                    changeAudioTrack(t.id);
                    setOpen(false);
                  }}
                  className={`flex w-full items-center gap-2 px-3 py-2 text-left text-xs ${
                    hasMultipleTracks
                      ? "cursor-pointer hover:bg-white/10"
                      : "cursor-default"
                  } ${t.selected ? "text-[var(--accent)]" : "text-white/90"}`}
                >
                  <span className="w-3 flex-shrink-0">
                    {t.selected ? "✓" : ""}
                  </span>
                  <span className="min-w-0 flex-1">
                    {renderAudioTrackLabel(t, locale)}
                  </span>
                </button>
              ))
            )}
          </div>,
          document.body,
        )}
    </div>
  );
});

AudioTrackMenu.displayName = "AudioTrackMenu";

// ── Main overlay ─────────────────────────────────────────────────────────────

const OverlayToggleButton = memo(function OverlayToggleButton() {
  const paused = useVideoPlaybackSelector((state) => state.paused);
  const play = useVideoPlaybackSelector((state) => state.play);
  const pause = useVideoPlaybackSelector((state) => state.pause);

  const togglePlayback = useCallback(() => {
    if (paused) {
      play();
    } else {
      pause();
    }
  }, [pause, paused, play]);

  return (
    <button
      type="button"
      data-floating-surface="true"
      className={`absolute inset-0 focus:outline-none ${
        paused ? "cursor-pointer" : "cursor-default"
      }`}
      aria-label={paused ? "播放" : "暂停"}
      tabIndex={-1}
      onClick={togglePlayback}
    >
      {paused && (
        <div className="flex h-full w-full items-center justify-center">
          <div className="flex h-16 w-16 items-center justify-center rounded-full bg-black/40 text-white/70 transition-colors hover:bg-black/60 hover:text-white">
            <svg
              className="h-8 w-8 translate-x-0.5"
              viewBox="0 0 24 24"
              fill="currentColor"
            >
              <path d="M8 5v14l11-7z" />
            </svg>
          </div>
        </div>
      )}
    </button>
  );
});

OverlayToggleButton.displayName = "OverlayToggleButton";

const PlaybackStatusOverlay = memo(function PlaybackStatusOverlay() {
  const paused = useVideoPlaybackSelector((state) => state.paused);
  const waiting = useVideoPlaybackSelector((state) => state.waiting);
  const started = useVideoPlaybackSelector((state) => state.started);

  // Only show spinner when actively loading (not paused).
  // When paused, the play button in OverlayToggleButton is shown instead.
  if (paused || (started && !waiting)) return null;

  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
      <div className="h-12 w-12 animate-spin rounded-full border-2 border-white/20 border-t-white" />
    </div>
  );
});

PlaybackStatusOverlay.displayName = "PlaybackStatusOverlay";

const SeekBarSection = memo(function SeekBarSection({
  chapters,
}: {
  chapters: ChapterOutput[];
}) {
  const currentTime = useVideoPlaybackSelector((state) => state.currentTime);
  const duration = useVideoPlaybackSelector((state) => state.duration);
  const bufferedRanges = useVideoPlaybackSelector(
    (state) => state.bufferedRanges,
  );
  const seek = useVideoPlaybackSelector((state) => state.seek);
  const stableOnSeek = useCallback((time: number) => seek(time), [seek]);

  return (
    <PlayerSeekBar
      currentTime={currentTime}
      duration={duration}
      bufferedRanges={bufferedRanges}
      onSeek={stableOnSeek}
      chapters={chapters}
      className="group/seek mx-2 flex h-5 cursor-pointer items-center focus:outline-none"
    />
  );
});

SeekBarSection.displayName = "SeekBarSection";

const PlayPauseButton = memo(function PlayPauseButton() {
  const paused = useVideoPlaybackSelector((state) => state.paused);
  const play = useVideoPlaybackSelector((state) => state.play);
  const pause = useVideoPlaybackSelector((state) => state.pause);

  return (
    <PlayerControlTooltip title={paused ? "播放" : "暂停"}>
      <button
        type="button"
        className="flex h-8 w-8 cursor-pointer items-center justify-center rounded text-white hover:bg-white/10"
        aria-label={paused ? "播放" : "暂停"}
        onClick={() => {
          if (paused) {
            play();
          } else {
            pause();
          }
        }}
      >
        {paused ? (
          <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
        ) : (
          <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
          </svg>
        )}
      </button>
    </PlayerControlTooltip>
  );
});

PlayPauseButton.displayName = "PlayPauseButton";

const SkipControls = memo(function SkipControls() {
  const currentTime = useVideoPlaybackSelector((state) => state.currentTime);
  const duration = useVideoPlaybackSelector((state) => state.duration);
  const seek = useVideoPlaybackSelector((state) => state.seek);

  return (
    <>
      <PlayerControlTooltip title="后退10秒">
        <button
          type="button"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded text-white/80 hover:bg-white/10 hover:text-white"
          aria-label="后退10秒"
          onClick={() => seek(Math.max(0, currentTime - 10))}
        >
          <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M11.99 5V1l-5 5 5 5V7c3.31 0 6 2.69 6 6s-2.69 6-6 6-6-2.69-6-6h-2c0 4.42 3.58 8 8 8s8-3.58 8-8-3.58-8-8-8z" />
            <text
              x="7.5"
              y="15.5"
              fontSize="5.5"
              fontWeight="bold"
              fill="currentColor"
            >
              10
            </text>
          </svg>
        </button>
      </PlayerControlTooltip>

      <PlayerControlTooltip title="前进10秒">
        <button
          type="button"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded text-white/80 hover:bg-white/10 hover:text-white"
          aria-label="前进10秒"
          onClick={() => seek(Math.min(duration, currentTime + 10))}
        >
          <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M18 13c0 3.31-2.69 6-6 6s-6-2.69-6-6 2.69-6 6-6v4l5-5-5-5v4c-4.42 0-8 3.58-8 8s3.58 8 8 8 8-3.58 8-8h-2z" />
            <text
              x="7.5"
              y="15.5"
              fontSize="5.5"
              fontWeight="bold"
              fill="currentColor"
            >
              10
            </text>
          </svg>
        </button>
      </PlayerControlTooltip>
    </>
  );
});

SkipControls.displayName = "SkipControls";

const PlaybackTimeDisplay = memo(function PlaybackTimeDisplay() {
  const currentTime = useVideoPlaybackSelector((state) => state.currentTime);
  const duration = useVideoPlaybackSelector((state) => state.duration);

  return (
    <span className="ml-1 select-none whitespace-nowrap text-xs text-white/80">
      {fmtTime(currentTime)}
      {duration > 0 && ` / ${fmtTime(duration)}`}
    </span>
  );
});

PlaybackTimeDisplay.displayName = "PlaybackTimeDisplay";

const ToolbarStaticControls = memo(function ToolbarStaticControls({
  isFullscreen,
}: {
  isFullscreen: boolean;
}) {
  const { containerRef } = useVideoUiState();

  return (
    <>
      <VolumeControl />
      <div className="flex-1" />
      <ToneMappingToggle />
      <AudioTrackMenu />
      <SubtitleMenu />

      <PlayerControlTooltip title={isFullscreen ? "退出全屏" : "全屏"}>
        <button
          type="button"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded text-white/80 hover:bg-white/10 hover:text-white"
          aria-label={isFullscreen ? "退出全屏" : "全屏"}
          onClick={() => {
            const el = containerRef.current;
            if (!el) return;
            if (document.fullscreenElement === el) {
              document.exitFullscreen();
            } else {
              el.requestFullscreen();
            }
          }}
        >
          <svg
            className="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.8}
          >
            {isFullscreen ? (
              <path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3" />
            ) : (
              <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3" />
            )}
          </svg>
        </button>
      </PlayerControlTooltip>
    </>
  );
});

ToolbarStaticControls.displayName = "ToolbarStaticControls";

const PlaybackToolbarShell = memo(function PlaybackToolbarShell({
  visible,
  chapters,
  isFullscreen,
  pinControls,
  unpinControls,
}: {
  visible: boolean;
  chapters: ChapterOutput[];
  isFullscreen: boolean;
  pinControls: () => void;
  unpinControls: () => void;
}) {
  return (
    <div
      role="toolbar"
      className={`relative bg-gradient-to-t from-black/80 via-black/20 to-transparent px-1 pb-1 pt-10 transition-opacity duration-200 ${
        visible ? "opacity-100" : "pointer-events-none opacity-0"
      }`}
      onContextMenu={(e) => e.stopPropagation()}
      onMouseEnter={() => pinControls()}
      onMouseLeave={() => unpinControls()}
    >
      <SeekBarSection chapters={chapters} />

      <div className="mt-0.5 flex items-center gap-0.5 px-1">
        <PlayPauseButton />
        <SkipControls />
        <PlaybackTimeDisplay />
        <ToolbarStaticControls isFullscreen={isFullscreen} />
      </div>
    </div>
  );
});

PlaybackToolbarShell.displayName = "PlaybackToolbarShell";

export function CustomVideoControls() {
  const { toggleStats } = useVideoUiState();
  const {
    item,
    isMinimized,
    isFullscreen,
    controlsVisible: visible,
    showControls,
    pinControls,
    unpinControls,
    hideControlsNow,
  } = usePlayer();
  const { open: openCtxMenu, contextMenu: ctxMenuPortal } = useContextMenu();

  useEffect(() => {
    showControls();
  }, [showControls]);

  // 最小化模式下不渲染控制层，由 MiniPlayer 的控制条接管
  if (isMinimized) return null;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: video player mouse-tracking overlay
    <div
      className="absolute inset-0 z-20 flex select-none flex-col justify-end"
      style={{ cursor: visible ? "default" : "none" }}
      onMouseMove={() => showControls()}
      onMouseLeave={(e) => {
        // Don't hide if mouse moved to title bar (sibling within same window)
        const related = e.relatedTarget;
        const win = (e.currentTarget as HTMLElement).closest(
          "[data-window-id]",
        );
        if (win && related instanceof Node && win.contains(related)) return;
        hideControlsNow();
      }}
      onContextMenu={(e) => {
        openCtxMenu(e, [
          {
            key: "stats",
            label: "视频统计信息",
            icon: <BarChart3 size={13} />,
            onClick: toggleStats,
          },
        ]);
      }}
    >
      {ctxMenuPortal}
      <OverlayToggleButton />
      <PlaybackStatusOverlay />
      <VideoPlayerTitleBar visible={visible} />
      <PlaybackToolbarShell
        visible={visible}
        chapters={item?.file.chapters ?? []}
        isFullscreen={isFullscreen}
        pinControls={pinControls}
        unpinControls={unpinControls}
      />
    </div>
  );
}
