/**
 * VideoViewer — Window content adapter for video files.
 *
 * For sourceType "player", renders PlayerWindowShell which manages its own
 * window chrome (title bar, background, controls visibility) via useWindowFrame().
 * For regular files, renders VfsVideoViewer with standard HTML5 video controls.
 */

import { Info, Minus, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { buildFileUrl } from "@/apps/finder/components/types";
import { getWindowIcon } from "@/shared/components/icons/WindowIcon";
import { VideoPlayer } from "@/shell/player/VideoPlayer";
import {
  usePlayer,
  useThemeCore,
  useWindowActions,
  useWindowFrame,
  type WindowState,
} from "@/system";
import { buildSshFileUrl } from "../file-url";
import { SiblingFileList } from "../SiblingFileList";
import { FileProbePanel } from "./FileProbePanel";

export default function VideoViewer({ win }: { win: WindowState }) {
  const filePath = win.metadata.filePath ?? "";
  const fileSystemId = win.metadata.fileSystemId ?? "";

  if (win.sourceType === "player") {
    return <PlayerWindowShell />;
  }

  return (
    <VfsVideoViewer win={win} filePath={filePath} fileSystemId={fileSystemId} />
  );
}

// ── Player Window Shell ──────────────────────────────────────────────────────

const TITLEBAR_HEIGHT = 36;

/**
 * Custom window chrome for the integrated video player.
 * Renders its own title bar, background, and controls visibility —
 * FloatingWindow provides only the positioning shell.
 */
function PlayerWindowShell() {
  const frame = useWindowFrame();
  const player = usePlayer();
  const { isMacStyle } = useThemeCore();

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: player window shell
    <div
      className="relative h-full bg-black/90 backdrop-blur-2xl border border-white/[0.08] rounded-[inherit]"
      onMouseMove={player.showControls}
      onMouseLeave={player.hideControlsNow}
    >
      {/* ── Title bar overlay with drag + window controls ── */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: title bar drag surface */}
      <div
        className={`absolute inset-x-0 top-0 z-30 flex items-center select-none bg-gradient-to-b from-black/60 to-transparent text-white/90 transition-opacity duration-200 ${
          player.controlsVisible
            ? "pointer-events-auto opacity-100"
            : "pointer-events-none opacity-0"
        }`}
        style={{ height: TITLEBAR_HEIGHT }}
        onPointerDown={frame.onDragPointerDown}
        onPointerMove={frame.onDragPointerMove}
        onPointerUp={frame.onDragPointerUp}
        onDoubleClick={frame.onDragDoubleClick}
        onMouseEnter={player.pinControls}
        onMouseLeave={player.unpinControls}
      >
        {isMacStyle ? (
          <>
            <div className="flex items-center gap-1.5 pl-3 shrink-0">
              {/* Close — red */}
              <button
                type="button"
                className="group/dot relative w-3 h-3 rounded-full bg-[#ff5f57] hover:brightness-90 transition-all cursor-pointer flex items-center justify-center"
                onClick={frame.close}
              >
                <svg
                  className="w-[7px] h-[7px] opacity-0 group-hover/dot:opacity-100 transition-opacity"
                  viewBox="0 0 10 10"
                  fill="none"
                  stroke="#820005"
                  strokeWidth={1.8}
                >
                  <path d="M2 2l6 6M8 2l-6 6" />
                </svg>
              </button>
              {/* Minimize — yellow */}
              <button
                type="button"
                className="group/dot relative w-3 h-3 rounded-full bg-[#febc2e] hover:brightness-90 transition-all cursor-pointer flex items-center justify-center"
                onClick={frame.minimize}
              >
                <svg
                  className="w-[7px] h-[7px] opacity-0 group-hover/dot:opacity-100 transition-opacity"
                  viewBox="0 0 10 2"
                  fill="none"
                  stroke="#7B5700"
                  strokeWidth={1.8}
                >
                  <path d="M1 1h8" />
                </svg>
              </button>
              {/* Maximize — green */}
              <button
                type="button"
                className="group/dot relative w-3 h-3 rounded-full bg-[#28c840] hover:brightness-90 transition-all cursor-pointer flex items-center justify-center"
                onClick={frame.toggleMaximize}
              >
                <svg
                  className="w-[7px] h-[7px] opacity-0 group-hover/dot:opacity-100 transition-opacity"
                  viewBox="0 0 10 10"
                  fill="none"
                  stroke="#006500"
                  strokeWidth={1.8}
                >
                  {frame.win.maximized ? (
                    <path d="M6 1h3v3M4 9H1V6M9 1L5.5 4.5M1 9l3.5-3.5" />
                  ) : (
                    <path d="M1 4V1h3M9 6v3H6M1 1l3.5 3.5M9 9L5.5 5.5" />
                  )}
                </svg>
              </button>
            </div>
            <span className="flex-1 min-w-0 text-xs font-medium truncate text-center px-2 flex items-center justify-center gap-1.5">
              <span className="shrink-0">
                {getWindowIcon(frame.win.type, 16, frame.win.metadata.fileName)}
              </span>
              {frame.win.title}
            </span>
            <div className="w-[60px] shrink-0" />
          </>
        ) : (
          <>
            <span className="flex-1 min-w-0 text-xs font-medium truncate pl-3 flex items-center gap-1.5">
              <span className="shrink-0">
                {getWindowIcon(frame.win.type, 16, frame.win.metadata.fileName)}
              </span>
              {frame.win.title}
            </span>
            <div className="flex items-center shrink-0 h-full">
              <button
                type="button"
                className="flex items-center justify-center w-[46px] h-full hover:bg-white/10 transition-colors cursor-pointer"
                onClick={frame.minimize}
              >
                <Minus size={12} />
              </button>
              <button
                type="button"
                className="flex items-center justify-center w-[46px] h-full hover:bg-white/10 transition-colors cursor-pointer"
                onClick={frame.toggleMaximize}
              >
                {frame.win.maximized ? (
                  <svg
                    width="11"
                    height="11"
                    viewBox="0 0 10 10"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={1}
                  >
                    <rect x="2" y="3" width="6.5" height="6.5" rx="0.5" />
                    <path d="M3.5 3V1.5a.5.5 0 0 1 .5-.5H9a.5.5 0 0 1 .5.5V6a.5.5 0 0 1-.5.5H8.5" />
                  </svg>
                ) : (
                  <svg
                    width="11"
                    height="11"
                    viewBox="0 0 10 10"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={1}
                  >
                    <rect x="0.5" y="0.5" width="9" height="9" rx="0.5" />
                  </svg>
                )}
              </button>
              <button
                type="button"
                className="flex items-center justify-center w-[46px] h-full hover:bg-red-500 hover:text-white transition-colors cursor-pointer"
                onClick={frame.close}
              >
                <X size={12} />
              </button>
            </div>
          </>
        )}
      </div>

      <VideoPlayer />
    </div>
  );
}

// ── VFS Video Viewer ─────────────────────────────────────────────────────────

/** Interval (ms) for saving playback position to window metadata. */
const SAVE_INTERVAL = 3_000;

function VfsVideoViewer({
  win,
  filePath,
  fileSystemId,
}: {
  win: WindowState;
  filePath: string;
  fileSystemId: string;
}) {
  const [showInfo, setShowInfo] = useState(false);
  const fileName = win.metadata.fileName ?? win.title;
  const videoRef = useRef<HTMLVideoElement>(null);
  const { updateMetadata } = useWindowActions();

  const videoSrc =
    buildFileUrl(filePath, fileSystemId) ??
    buildSshFileUrl(win.metadata.sshTerminalId, filePath);

  // Saved position from metadata (read once per file)
  const savedPosition = useRef(win.metadata.playbackPosition ?? 0);

  // When src changes (sibling switch), pause old → save → load new
  const prevSrc = useRef(videoSrc);
  useEffect(() => {
    const el = videoRef.current;
    if (!el || !videoSrc) return;
    if (prevSrc.current !== videoSrc) {
      // Save position for old file before switching
      if (el.duration) {
        updateMetadata(win.id, {
          playbackPosition: Math.floor(el.currentTime),
        });
      }
      // Reset saved position for new file (no saved position yet)
      savedPosition.current = 0;
      prevSrc.current = videoSrc;
      el.pause();
      el.src = videoSrc;
      el.load();
    }
  }, [videoSrc, win.id, updateMetadata]);

  // Restore position once video metadata is loaded
  const handleLoadedMetadata = useCallback(() => {
    const el = videoRef.current;
    if (!el) return;
    const pos = savedPosition.current;
    if (pos > 0 && pos < el.duration - 1) {
      el.currentTime = pos;
    }
    el.play().catch(() => {});
  }, []);

  // Periodically save playback position to window metadata
  useEffect(() => {
    const timer = setInterval(() => {
      const el = videoRef.current;
      if (!el || el.paused || el.ended) return;
      updateMetadata(win.id, {
        playbackPosition: Math.floor(el.currentTime),
        playbackDuration: Math.floor(el.duration) || undefined,
      });
    }, SAVE_INTERVAL);
    return () => clearInterval(timer);
  }, [win.id, updateMetadata]);

  // Save position on pause / before unmount
  const savePosition = useCallback(() => {
    const el = videoRef.current;
    if (!el?.duration) return;
    updateMetadata(win.id, {
      playbackPosition: Math.floor(el.currentTime),
      playbackDuration: Math.floor(el.duration) || undefined,
    });
  }, [win.id, updateMetadata]);

  useEffect(() => {
    return () => savePosition();
  }, [savePosition]);

  return (
    <div className="flex h-full">
      {/* ── Main video area ───────────────────────────────────── */}
      <div className="relative flex flex-1 flex-col min-w-0">
        <div className="flex min-h-0 flex-1 items-center justify-center bg-black p-2">
          {videoSrc && (
            // biome-ignore lint/a11y/useMediaCaption: file preview
            <video
              ref={videoRef}
              controls
              autoPlay
              onLoadedMetadata={handleLoadedMetadata}
              onPause={savePosition}
              className="max-h-full max-w-full rounded"
            />
          )}
        </div>

        {/* ── Bottom bar with info toggle ─────────────────────── */}
        <div className="flex h-8 shrink-0 items-center justify-end gap-1 border-t border-border-base bg-surface-elevated px-3 ">
          {fileSystemId && (
            <SiblingFileList
              windowId={win.id}
              fileSystemId={fileSystemId}
              filePath={filePath}
              kind="video"
            />
          )}
          <button
            type="button"
            onClick={() => setShowInfo((v) => !v)}
            title="详细信息"
            className={`rounded p-1 transition-colors ${
              showInfo
                ? "bg-blue-100 text-blue-600 dark:bg-blue-900/40 dark:text-blue-400"
                : "text-fg-muted hover:bg-fill-tertiary hover:text-fg-secondary"
            }`}
          >
            <Info className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* ── Detail panel (right) ──────────────────────────────── */}
      {showInfo && fileSystemId && (
        <div className="w-[280px] shrink-0 border-l border-border-base bg-surface-elevated ">
          <FileProbePanel
            fileSystemId={fileSystemId}
            filePath={filePath}
            fileName={fileName}
          />
        </div>
      )}
    </div>
  );
}
