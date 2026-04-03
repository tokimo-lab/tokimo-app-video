/**
 * VideoStatsPanel — YouTube-style "Stats for nerds" overlay
 *
 * Displays real-time playback statistics like resolution, codecs,
 * buffer health, dropped frames, etc.
 */
import type Hls from "hls.js";
import { X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { PlayingItem } from "@/system";

interface VideoStatsEntry {
  label: string;
  value: string;
}

interface VideoStatsPanelProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  hlsRef: React.RefObject<Hls | null>;
  item: PlayingItem;
  onClose: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / k ** i).toFixed(1)} ${sizes[i]}`;
}

function formatBitrate(bps: number): string {
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(0)} Kbps`;
  return `${bps} bps`;
}

function collectStats(
  video: HTMLVideoElement,
  hls: Hls | null,
  item: PlayingItem,
): VideoStatsEntry[] {
  const entries: VideoStatsEntry[] = [];

  // Player type
  let playerType = "Native HTML5";
  if (hls) playerType = "HLS.js";
  else if (item.streamUrl.toLowerCase().endsWith(".flv")) playerType = "FLV.js";
  entries.push({ label: "Player", value: playerType });

  // Viewport (display dimensions)
  entries.push({
    label: "Viewport",
    value: `${video.clientWidth}×${video.clientHeight}`,
  });

  // Video resolution (decoded dimensions)
  if (video.videoWidth && video.videoHeight) {
    const fileMeta = item.file;
    let resValue = `${video.videoWidth}×${video.videoHeight}`;
    if (fileMeta.hdrType) {
      const HDR_LABEL: Record<string, string> = {
        hdr10: "HDR10",
        hdr10plus: "HDR10+",
        hdr10_plus: "HDR10+",
        hlg: "HLG",
        dolby_vision: "Dolby Vision",
        dovi: "Dolby Vision",
        dolby_vision_hdr10: "DV + HDR10",
        dolby_vision_hdr10_plus: "DV + HDR10+",
        dolby_vision_hlg: "DV + HLG",
        dolby_vision_sdr: "DV + SDR",
        dolby_vision_el: "DV (EL)",
        dolby_vision_el_hdr10_plus: "DV (EL) + HDR10+",
        dovi_invalid: "DV (Invalid)",
        sdr: "SDR",
      };
      const hdr =
        HDR_LABEL[fileMeta.hdrType.toLowerCase()] ??
        fileMeta.hdrType.toUpperCase();
      resValue += ` (${hdr})`;
    }
    entries.push({ label: "Resolution", value: resValue });
  }

  // Codecs
  const videoCodec = item.file.videoCodec;
  const videoProfile = item.file.videoProfile;
  let codecDisplay = videoCodec ?? "unknown";
  if (videoProfile) codecDisplay += ` (${videoProfile})`;
  entries.push({ label: "Video Codec", value: codecDisplay });

  // Audio codec — from selected audio track or file metadata
  const audioStreams = item.file.audioStreams as
    | Array<{
        codec_name?: string;
        tags?: { language?: string };
        channels?: number;
      }>
    | null
    | undefined;
  if (audioStreams && audioStreams.length > 0) {
    const firstAudio = audioStreams[0];
    let audioDisplay = firstAudio.codec_name ?? "unknown";
    if (firstAudio.channels) {
      audioDisplay += `, ${firstAudio.channels}ch`;
    }
    entries.push({ label: "Audio Codec", value: audioDisplay });
  }

  // HLS level info
  if (hls) {
    const currentLevel = hls.currentLevel;
    const levels = hls.levels;
    if (currentLevel >= 0 && levels[currentLevel]) {
      const level = levels[currentLevel];
      if (level.bitrate) {
        entries.push({
          label: "Bitrate (HLS)",
          value: formatBitrate(level.bitrate),
        });
      }
      if (level.codecSet) {
        entries.push({ label: "HLS Codecs", value: level.codecSet });
      }
    }
  }

  // Dropped frames
  if (typeof video.getVideoPlaybackQuality === "function") {
    const quality = video.getVideoPlaybackQuality();
    entries.push({
      label: "Frames",
      value: `${quality.droppedVideoFrames} dropped / ${quality.totalVideoFrames} total`,
    });
  }

  // Buffer health
  let bufferHealth = 0;
  if (video.buffered.length > 0) {
    for (let i = 0; i < video.buffered.length; i++) {
      const end = video.buffered.end(i);
      if (end > video.currentTime) {
        bufferHealth = end - video.currentTime;
        break;
      }
    }
  }
  entries.push({
    label: "Buffer Health",
    value: `${bufferHealth.toFixed(1)}s`,
  });

  // Volume
  entries.push({
    label: "Volume",
    value: video.muted ? "Muted" : `${Math.round(video.volume * 100)}%`,
  });

  // File info
  if (item.file.size) {
    entries.push({ label: "File Size", value: formatBytes(item.file.size) });
  }

  entries.push({ label: "File", value: item.file.filename });

  return entries;
}

export function VideoStatsPanel({
  videoRef,
  hlsRef,
  item,
  onClose,
}: VideoStatsPanelProps) {
  const [stats, setStats] = useState<VideoStatsEntry[]>([]);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setStats(collectStats(video, hlsRef.current, item));
  }, [videoRef, hlsRef, item]);

  useEffect(() => {
    refresh();
    timerRef.current = setInterval(refresh, 500);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [refresh]);

  return (
    <div className="pointer-events-auto absolute left-3 top-12 z-30 max-w-[420px] rounded-lg bg-black/80 p-3 font-mono text-xs text-white/90 shadow-xl backdrop-blur-sm">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] font-semibold tracking-wide text-white/60">
          Stats for Nerds
        </span>
        <button
          type="button"
          onClick={onClose}
          className="flex h-5 w-5 items-center justify-center rounded-full text-white/50 transition-colors hover:bg-white/10 hover:text-white"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="space-y-0.5">
        {stats.map((entry) => (
          <div key={entry.label} className="flex gap-2">
            <span className="shrink-0 text-white/50">{entry.label}:</span>
            <span className="break-all">{entry.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
