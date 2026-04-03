/**
 * VideoPlayer — 原生 HTML5 播放器（编排层）
 *
 * 重型逻辑已拆分到专用 hook：
 *   useVideoEngine   — HLS / FLV / DirectPlay 引擎生命周期 + 音轨管理
 *   useVideoEvents   — HTML5 video 事件监听 → React 播放状态
 *   useSubtitleManager — 字幕全生命周期（native VTT / ASS / PGS / SSE）
 *
 * 此文件只负责：
 *   1. 持有共享的 DOM refs + 播放状态
 *   2. 组合三个 hook
 *   3. PlayerContext ↔ video 元素的薄桥接（play/pause sync, volume, fullscreen, seekHandler）
 *   4. 构建 VideoStateContext 并渲染 JSX
 */
import type { CSSProperties, SetStateAction } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getTextShadowCss,
  loadSubtitleStyleSettings,
  type SubtitleStyleSettings,
} from "@/lib/player-subtitles";
import { useUiPreference } from "@/lib/use-preference";
import {
  createVideoPlaybackStore,
  usePlayer,
  usePlayerPosition,
  type VideoPlaybackStore,
  VideoStateProvider,
  type VideoTrackStateValue,
  type VideoUiStateValue,
  type VideoVolumeStateValue,
} from "@/system";
import type { PlayerPrefs, SubtitleOutput } from "@/types";
import { SubtitleCustomOverlay } from "../subtitle/SubtitleCustomOverlay";
import { useSubtitleManager } from "../subtitle/useSubtitleManager";
import { CustomVideoControls } from "./CustomVideoControls";
import { useVideoEngine } from "./useVideoEngine";
import { useVideoEvents } from "./useVideoEvents";
import { VideoStatsPanel } from "./VideoStatsPanel";

const EMPTY_SUBTITLES: SubtitleOutput[] = [];

// ── Component ─────────────────────────────────────────────────────────────────

export function VideoPlayer() {
  const {
    item,
    isPlaying,
    setIsPlaying,
    setIsFullscreen,
    setSeekHandler,
    setPosition,
    seekTo: requestSeekTo,
    changeStreamAudioTrack,
    volume: _playerVolume,
    setVolume: setPlayerVolume,
    videoSetVolumeRef,
  } = usePlayer();

  // ── DOM refs ──
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  // ── Playback state (shared across hooks via params) ──
  const [muted, setMuted] = useState(false);
  const [volume, setVolume] = useState(1);
  const [showStats, setShowStats] = useState(false);
  const pendingSeekTimeRef = useRef<number | null>(null);

  // ── Stable primitives ──
  const streamUrl = item?.streamUrl ?? null;
  const filename = item?.file.filename ?? "";
  const itemFileId = item?.fileId ?? null;
  const itemResumePosition = item?.resumePosition ?? 0;
  const itemDuration = item?.duration ?? null;
  const itemSubtitles = item?.subtitles ?? EMPTY_SUBTITLES;

  const play = useCallback(() => {
    setIsPlaying(true);
  }, [setIsPlaying]);

  const pause = useCallback(() => {
    setIsPlaying(false);
  }, [setIsPlaying]);

  const seek = useCallback(
    (time: number) => {
      requestSeekTo(Math.max(0, time));
    },
    [requestSeekTo],
  );

  const playbackStoreRef = useRef<VideoPlaybackStore | null>(null);
  if (!playbackStoreRef.current) {
    playbackStoreRef.current = createVideoPlaybackStore({
      currentTime: itemResumePosition > 10 ? itemResumePosition : 0,
      duration: itemDuration ?? 0,
      paused: true,
      waiting: false,
      started: false,
      bufferedRanges: [],
      play,
      pause,
      seek,
    });
  }
  const playbackStore = playbackStoreRef.current;

  const updatePlaybackStore = useCallback(
    (updater: SetStateAction<ReturnType<VideoPlaybackStore["getState"]>>) => {
      playbackStore.setState(updater);
    },
    [playbackStore],
  );

  const setCurrentTime = useCallback(
    (next: SetStateAction<number>) => {
      updatePlaybackStore((prev) => {
        const currentTime =
          typeof next === "function" ? next(prev.currentTime) : next;
        return Object.is(prev.currentTime, currentTime)
          ? prev
          : { ...prev, currentTime };
      });
    },
    [updatePlaybackStore],
  );

  const setDuration = useCallback(
    (next: SetStateAction<number>) => {
      updatePlaybackStore((prev) => {
        const duration =
          typeof next === "function" ? next(prev.duration) : next;
        return Object.is(prev.duration, duration)
          ? prev
          : { ...prev, duration };
      });
    },
    [updatePlaybackStore],
  );

  const setPaused = useCallback(
    (next: SetStateAction<boolean>) => {
      updatePlaybackStore((prev) => {
        const paused = typeof next === "function" ? next(prev.paused) : next;
        return Object.is(prev.paused, paused) ? prev : { ...prev, paused };
      });
    },
    [updatePlaybackStore],
  );

  const setWaiting = useCallback(
    (next: SetStateAction<boolean>) => {
      updatePlaybackStore((prev) => {
        const waiting = typeof next === "function" ? next(prev.waiting) : next;
        return Object.is(prev.waiting, waiting) ? prev : { ...prev, waiting };
      });
    },
    [updatePlaybackStore],
  );

  const setStarted = useCallback(
    (next: SetStateAction<boolean>) => {
      updatePlaybackStore((prev) => {
        const started = typeof next === "function" ? next(prev.started) : next;
        return Object.is(prev.started, started) ? prev : { ...prev, started };
      });
    },
    [updatePlaybackStore],
  );

  const setBufferedRanges = useCallback(
    (next: SetStateAction<{ start: number; end: number }[]>) => {
      updatePlaybackStore((prev) => {
        const bufferedRanges =
          typeof next === "function" ? next(prev.bufferedRanges) : next;
        return Object.is(prev.bufferedRanges, bufferedRanges)
          ? prev
          : { ...prev, bufferedRanges };
      });
    },
    [updatePlaybackStore],
  );

  // ── Subtitle style initialisation ──
  const playerPref = useUiPreference<PlayerPrefs>("player");
  const initialSubtitleSettings =
    (playerPref.data?.subtitleSettings as SubtitleStyleSettings | undefined) ??
    loadSubtitleStyleSettings();

  // ── Subtitle manager (owns subtitle state + renderers) ──
  const subtitleManager = useSubtitleManager({
    videoRef,
    streamUrl,
    itemFileId,
    itemSubtitles,
    initialSubtitleSettings,
  });

  // ── Video engine (owns HLS/FLV/DirectPlay + audio tracks) ──
  const engine = useVideoEngine({
    videoRef,
    streamUrl,
    filename,
    itemFileId,
    itemDuration,
    itemAudioStreams: item?.file.audioStreams ?? null,
    itemResumePosition,
    isPlaying,
    setNativeTextTracks: subtitleManager.setNativeTextTracks,
    setStarted,
    setDuration,
    setCurrentTime,
    setBufferedRanges,
    setWaiting,
    pendingSeekTimeRef,
    setPosition,
    changeStreamAudioTrack,
  });

  // ── Video events (binds HTML5 events → React state) ──
  useVideoEvents({
    videoRef,
    pendingSeekTimeRef,
    itemDuration,
    setCurrentTime,
    setDuration,
    setPaused,
    setMuted,
    setVolume,
    setWaiting,
    setStarted,
    setBufferedRanges,
    setIsPlaying,
    setPosition,
  });

  // ── Volume bridging with PlayerContext ──
  useEffect(() => {
    setPlayerVolume(volume);
  }, [volume, setPlayerVolume]);

  const changeVolumeRef = useRef<((v: number) => void) | null>(null);
  useEffect(() => {
    videoSetVolumeRef.current = (v: number) => {
      changeVolumeRef.current?.(v);
    };
    return () => {
      videoSetVolumeRef.current = null;
    };
  }, [videoSetVolumeRef]);

  // ── Browser Fullscreen sync ──
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const handler = () => {
      setIsFullscreen(document.fullscreenElement === el);
    };
    document.addEventListener("fullscreenchange", handler);
    return () => document.removeEventListener("fullscreenchange", handler);
  }, [setIsFullscreen]);

  // ── PlayerContext: seekHandler ──
  useEffect(() => {
    setSeekHandler((pos) => {
      const nextTime = Math.max(0, pos);
      pendingSeekTimeRef.current = nextTime;
      setCurrentTime(nextTime);
      if (videoRef.current) {
        videoRef.current.currentTime = nextTime;
      }
    });
    return () => setSeekHandler(null);
  }, [setCurrentTime, setSeekHandler]);

  // ── PlayerContext: play / pause sync ──
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !itemFileId) return;
    if (isPlaying) {
      video.play().catch(() => {});
    } else {
      video.pause();
    }
  }, [isPlaying, itemFileId]);

  const mute = useCallback(() => {
    if (videoRef.current) videoRef.current.muted = true;
  }, []);

  const unmute = useCallback(() => {
    if (videoRef.current) videoRef.current.muted = false;
  }, []);

  const changeVolume = useCallback((v: number) => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = Math.max(0, Math.min(1, v));
    if (v > 0) video.muted = false;
  }, []);
  changeVolumeRef.current = changeVolume;

  const toggleStats = useCallback(() => {
    setShowStats((prev) => !prev);
  }, []);

  const volumeValue = useMemo<VideoVolumeStateValue>(
    () => ({
      muted,
      volume,
      mute,
      unmute,
      changeVolume,
    }),
    [changeVolume, mute, muted, unmute, volume],
  );

  const trackValue = useMemo<VideoTrackStateValue>(
    () => ({
      audioTracks: engine.audioTracks,
      subtitleTracks: subtitleManager.subtitleTracks,
      activeSubtitleId: subtitleManager.activeSubtitleId,
      subtitleStyleSettings: subtitleManager.subtitleStyleSettings,
      changeAudioTrack: engine.changeAudioTrack,
      setSubtitle: subtitleManager.setSubtitle,
      updateSubtitleStyleSettings: subtitleManager.updateSubtitleStyleSettings,
      registerSubtitleTrack: subtitleManager.registerSubtitleTrack,
      removeSubtitleTrack: subtitleManager.removeSubtitleTrack,
    }),
    [
      engine.audioTracks,
      engine.changeAudioTrack,
      subtitleManager.activeSubtitleId,
      subtitleManager.registerSubtitleTrack,
      subtitleManager.removeSubtitleTrack,
      subtitleManager.setSubtitle,
      subtitleManager.subtitleStyleSettings,
      subtitleManager.subtitleTracks,
      subtitleManager.updateSubtitleStyleSettings,
    ],
  );

  const uiValue = useMemo<VideoUiStateValue>(
    () => ({
      containerRef,
      showStats,
      toggleStats,
    }),
    [showStats, toggleStats],
  );

  useEffect(() => {
    playbackStore.setState((prev) => {
      const nextDuration = itemDuration ?? prev.duration;
      if (
        prev.play === play &&
        prev.pause === pause &&
        prev.seek === seek &&
        prev.duration === nextDuration
      ) {
        return prev;
      }

      return {
        ...prev,
        duration: nextDuration,
        play,
        pause,
        seek,
      };
    });
  }, [itemDuration, pause, play, playbackStore, seek]);

  // ── Render ──

  if (!item) return null;

  return (
    <VideoStateProvider
      playbackStore={playbackStore}
      volumeValue={volumeValue}
      trackValue={trackValue}
      uiValue={uiValue}
    >
      <div
        ref={containerRef}
        className="player-subtitle-host relative h-full w-full bg-black"
        style={
          {
            "--player-subtitle-color":
              subtitleManager.subtitleStyleSettings.color,
            "--player-subtitle-font-family":
              subtitleManager.subtitleStyleSettings.fontFamily,
            "--player-subtitle-font-size": `${subtitleManager.subtitleStyleSettings.fontSize}px`,
            "--player-subtitle-bg":
              subtitleManager.subtitleStyleSettings.backgroundColor,
            "--player-subtitle-weight":
              subtitleManager.subtitleStyleSettings.fontWeight,
            "--player-subtitle-shadow": getTextShadowCss(
              subtitleManager.subtitleStyleSettings.textShadow,
            ),
          } as CSSProperties
        }
      >
        <style>{`
          .player-subtitle-host video::cue {
            color: var(--player-subtitle-color);
            font-family: var(--player-subtitle-font-family);
            font-size: var(--player-subtitle-font-size);
            background: var(--player-subtitle-bg);
            font-weight: var(--player-subtitle-weight);
            text-shadow: var(--player-subtitle-shadow);
          }
          @keyframes playerLogIn {
            from { opacity: 0; transform: translateY(4px); }
            to   { opacity: 1; transform: translateY(0); }
          }
          .player-log-line {
            animation: playerLogIn 0.25s ease-out both;
          }
        `}</style>
        {/* biome-ignore lint/a11y/useMediaCaption: captions managed by subtitle engine */}
        <video
          ref={videoRef}
          className="absolute inset-0 h-full w-full object-contain"
          crossOrigin="use-credentials"
          autoPlay={isPlaying}
          playsInline
        />
        {subtitleManager.subtitleStyleSettings.renderMode === "custom" && (
          <SubtitleCustomOverlay
            videoRef={videoRef}
            settings={subtitleManager.subtitleStyleSettings}
          />
        )}
        <CustomVideoControls />
        {showStats && item && (
          <VideoStatsPanel
            videoRef={videoRef}
            hlsRef={engine.hlsRef}
            item={item}
            onClose={toggleStats}
          />
        )}
      </div>
    </VideoStateProvider>
  );
}

// ── Helper components ─────────────────────────────────────────────────────────

/** 迷你播放控制条中的播放状态展示（只读） */
export function PlayerCurrentTimeDisplay() {
  const { item } = usePlayer();
  const currentPosition = usePlayerPosition();
  if (!item) return null;
  const total = item.duration ?? 0;
  const fmt = (secs: number) => {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${m}:${String(s).padStart(2, "0")}`;
  };
  return (
    <span className="text-xs text-fg-muted">
      {fmt(currentPosition)} / {fmt(total)}
    </span>
  );
}
