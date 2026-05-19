/**
 * VideoViewer — Simple file-based video viewer.
 *
 * Opened when double-clicking a video file in the file manager.
 * Uses native HTML5 video controls for direct file playback.
 */

import { buildFileUrl } from "@tokimo/sdk";
import { Info } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useWindowActions, type WindowState } from "../../shell-shim/system";
import { buildAgentFileUrl, buildSshFileUrl } from "../file-url";
import { SiblingFileList } from "../SiblingFileList";
import { FileProbePanel } from "./FileProbePanel";

export default function VideoViewer({ win }: { win: WindowState }) {
  const filePath = win.metadata.filePath ?? "";
  const fileSystemId = win.metadata.fileSystemId ?? "";

  return (
    <VfsVideoViewer win={win} filePath={filePath} fileSystemId={fileSystemId} />
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
    win.metadata.directUrl ??
    buildFileUrl(filePath, fileSystemId) ??
    buildSshFileUrl(win.metadata.sshTerminalId, filePath) ??
    buildAgentFileUrl(win.metadata.agentId, filePath);

  // Saved position from metadata (read once per file)
  const savedPosition = useRef(win.metadata.playbackPosition ?? 0);

  // On sibling switch: save old position, reset for new file, reload
  const prevFilePath = useRef(filePath);
  useEffect(() => {
    if (prevFilePath.current === filePath) return;

    const el = videoRef.current;
    // Save position for old file before switching
    if (el?.duration) {
      updateMetadata(win.id, {
        playbackPosition: Math.floor(el.currentTime),
      });
    }
    savedPosition.current = 0;
    prevFilePath.current = filePath;

    // React already updated the src prop; force reload for browser compat
    if (el) el.load();
  }, [filePath, win.id, updateMetadata]);

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
              src={videoSrc}
              controls
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
