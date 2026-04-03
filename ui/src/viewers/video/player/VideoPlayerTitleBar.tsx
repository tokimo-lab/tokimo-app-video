/**
 * VideoPlayerTitleBar — OS-aware window title bar for the video player.
 *
 * - macOS: traffic-light buttons on the left, title centered
 * - Windows: Win11-style minimize / maximize / close on the right, title on the left
 * - Double-click the title bar area → toggle browser exclusive fullscreen
 * - Maximize / fullscreen button → browser exclusive fullscreen
 */
import { Tooltip } from "@tokiomo/components";
import type { ReactElement } from "react";
import { memo, useCallback } from "react";
import { usePlayer, useThemeCore, useVideoUiState } from "@/system";

function TitleBarTooltip({
  title,
  children,
}: {
  title: string;
  children: ReactElement;
}) {
  return (
    <Tooltip
      title={title}
      mouseEnterDelay={0}
      mouseLeaveDelay={0}
      color="bg-black/65 text-white backdrop-blur-2xl ring-1 ring-white/10"
    >
      {children}
    </Tooltip>
  );
}

// ── macOS traffic-light buttons ─────────────────────────────────────────────

function MacTrafficLights({
  isFullscreen,
  onClose,
  onMinimize,
  onFullscreen,
}: {
  isFullscreen: boolean;
  onClose: () => void;
  onMinimize: () => void;
  onFullscreen: () => void;
}) {
  return (
    <div className="pointer-events-auto flex items-center gap-[7px]">
      {/* Close — red */}
      <TitleBarTooltip title="关闭">
        <button
          type="button"
          aria-label="关闭"
          className="group/dot flex h-[13px] w-[13px] cursor-pointer items-center justify-center rounded-full bg-[#FF5F57] shadow-sm transition-transform hover:scale-110"
          onClick={onClose}
        >
          <svg
            className="invisible h-[7px] w-[7px] group-hover/dot:visible"
            viewBox="0 0 10 10"
            fill="none"
            stroke="#820005"
            strokeWidth={1.8}
          >
            <path d="M2 2l6 6M8 2l-6 6" />
          </svg>
        </button>
      </TitleBarTooltip>
      {/* Minimize — yellow */}
      <TitleBarTooltip title="最小化">
        <button
          type="button"
          aria-label="最小化"
          className="group/dot flex h-[13px] w-[13px] cursor-pointer items-center justify-center rounded-full bg-[#FEBC2E] shadow-sm transition-transform hover:scale-110"
          onClick={onMinimize}
        >
          <svg
            className="invisible h-[7px] w-[7px] group-hover/dot:visible"
            viewBox="0 0 10 2"
            fill="none"
            stroke="#7B5700"
            strokeWidth={1.8}
          >
            <path d="M1 1h8" />
          </svg>
        </button>
      </TitleBarTooltip>
      {/* Fullscreen — green */}
      <TitleBarTooltip title={isFullscreen ? "退出全屏" : "全屏"}>
        <button
          type="button"
          aria-label={isFullscreen ? "退出全屏" : "全屏"}
          className="group/dot flex h-[13px] w-[13px] cursor-pointer items-center justify-center rounded-full bg-[#28C840] shadow-sm transition-transform hover:scale-110"
          onClick={onFullscreen}
        >
          <svg
            className="invisible h-[7px] w-[7px] group-hover/dot:visible"
            viewBox="0 0 10 10"
            fill="none"
            stroke="#006500"
            strokeWidth={1.8}
          >
            {isFullscreen ? (
              <path d="M6 1h3v3M4 9H1V6M9 1L5.5 4.5M1 9l3.5-3.5" />
            ) : (
              <path d="M1 4V1h3M9 6v3H6M1 1l3.5 3.5M9 9L5.5 5.5" />
            )}
          </svg>
        </button>
      </TitleBarTooltip>
    </div>
  );
}

// ── Windows 11-style buttons ────────────────────────────────────────────────

function Win11Button({
  label,
  tooltip,
  onClick,
  hoverClass,
  children,
}: {
  label: string;
  tooltip: string;
  onClick: () => void;
  hoverClass: string;
  children: React.ReactNode;
}) {
  return (
    <TitleBarTooltip title={tooltip}>
      <button
        type="button"
        aria-label={label}
        className={`flex h-8 w-[46px] cursor-pointer items-center justify-center text-white/90 transition-colors ${hoverClass}`}
        onClick={onClick}
      >
        {children}
      </button>
    </TitleBarTooltip>
  );
}

function WinControls({
  isFullscreen,
  onClose,
  onMinimize,
  onFullscreen,
}: {
  isFullscreen: boolean;
  onClose: () => void;
  onMinimize: () => void;
  onFullscreen: () => void;
}) {
  return (
    <div className="pointer-events-auto -mr-1 flex items-center">
      {/* Minimize */}
      <Win11Button
        label="最小化"
        tooltip="最小化"
        onClick={onMinimize}
        hoverClass="hover:bg-white/10"
      >
        <svg width="10" height="1" viewBox="0 0 10 1" fill="currentColor">
          <rect width="10" height="1" />
        </svg>
      </Win11Button>
      {/* Maximize / Restore */}
      <Win11Button
        label={isFullscreen ? "还原" : "最大化"}
        tooltip={isFullscreen ? "还原" : "最大化"}
        onClick={onFullscreen}
        hoverClass="hover:bg-white/10"
      >
        {isFullscreen ? (
          // Restore icon — two overlapping rectangles
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            strokeWidth={1}
          >
            <rect x="2" y="3" width="6.5" height="6.5" rx="0.5" />
            <path d="M3.5 3V1.5a.5.5 0 0 1 .5-.5H9a.5.5 0 0 1 .5.5V6a.5.5 0 0 1-.5.5H8.5" />
          </svg>
        ) : (
          // Maximize icon — single rectangle
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            strokeWidth={1}
          >
            <rect x="0.5" y="0.5" width="9" height="9" rx="0.5" />
          </svg>
        )}
      </Win11Button>
      {/* Close */}
      <Win11Button
        label="关闭"
        tooltip="关闭"
        onClick={onClose}
        hoverClass="hover:bg-[#C42B1C] rounded-tr-xl"
      >
        <svg
          width="10"
          height="10"
          viewBox="0 0 10 10"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.2}
        >
          <path d="M1 1l8 8M9 1l-8 8" />
        </svg>
      </Win11Button>
    </div>
  );
}

// ── Main title bar ──────────────────────────────────────────────────────────

export const VideoPlayerTitleBar = memo(function VideoPlayerTitleBar({
  visible,
}: {
  visible: boolean;
}) {
  const { item, isFullscreen, setIsMinimized, closePlayer } = usePlayer();
  const { isMacStyle } = useThemeCore();
  const { containerRef } = useVideoUiState();

  const toggleMaximized = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    if (document.fullscreenElement === el) {
      document.exitFullscreen();
    } else {
      el.requestFullscreen();
    }
  }, [containerRef]);

  const handleMinimize = useCallback(() => {
    setIsMinimized(true);
  }, [setIsMinimized]);

  const handleClose = useCallback(() => {
    closePlayer();
  }, [closePlayer]);

  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      // Ignore double-click on buttons
      if ((e.target as HTMLElement).closest("button")) return;
      toggleMaximized();
    },
    [toggleMaximized],
  );

  // In windowed mode (not fullscreen), the FloatingWindow provides the title bar
  if (!isFullscreen) return null;

  const title = item?.title ?? "";

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: title bar double-click surface
    // biome-ignore lint/a11y/useKeyWithClickEvents: stopPropagation only, no keyboard equivalent needed
    <div
      className={`absolute inset-x-0 top-0 z-30 bg-gradient-to-b from-black/60 to-transparent transition-opacity duration-200 select-none ${
        visible
          ? "pointer-events-auto opacity-100"
          : "pointer-events-none opacity-0"
      }`}
      onDoubleClick={handleDoubleClick}
      onClick={(e) => e.stopPropagation()}
    >
      {isMacStyle ? (
        /* ── macOS layout: [traffic lights] ··· [centered title] ··· [spacer] ── */
        <div className="flex items-center px-4 pb-10 pt-3">
          <MacTrafficLights
            isFullscreen={isFullscreen}
            onClose={handleClose}
            onMinimize={handleMinimize}
            onFullscreen={toggleMaximized}
          />
          <span className="pointer-events-none flex-1 truncate text-center text-sm font-medium text-white drop-shadow-sm">
            {title}
          </span>
          {/* Spacer to balance the traffic lights for centering */}
          <div className="w-[55px] flex-shrink-0" />
        </div>
      ) : (
        /* ── Windows layout: [title] ··· [—  □  ✕] ── */
        <div className="flex items-center pl-4 pr-1 pb-10 pt-1">
          <span className="pointer-events-none flex-1 truncate pt-2 text-sm font-medium text-white drop-shadow-sm">
            {title}
          </span>
          <WinControls
            isFullscreen={isFullscreen}
            onClose={handleClose}
            onMinimize={handleMinimize}
            onFullscreen={toggleMaximized}
          />
        </div>
      )}
    </div>
  );
});

VideoPlayerTitleBar.displayName = "VideoPlayerTitleBar";
