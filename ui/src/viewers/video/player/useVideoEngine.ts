/**
 * useVideoEngine — manages video engine lifecycle (HLS / FLV / Mediabunny / DirectPlay).
 *
 * Owns engine refs (hlsRef, flvRef, mediabunny), audio track detection, native text track
 * sync, audio preference persistence, and resume-position coordination.
 */
import flvjs from "flv.js";
import Hls from "hls.js";
import type { MutableRefObject, RefObject, SetStateAction } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  loadAudioSelectionPreference,
  saveAudioSelectionPreference,
} from "@/lib/player-audio";
import { classifyAudioTrack } from "@/system/media/codec-detection";
import type { AudioTrackItem } from "@/system/media/VideoStateContext";
import { Ac3AudioPlayer } from "../audio/Ac3AudioPlayer";
import type { NativeTextTrackInfo } from "../subtitle/useSubtitleManager";

// ── Types ─────────────────────────────────────────────────────────────────────

type FlvPlayer = ReturnType<typeof flvjs.createPlayer>;

// ── Audio track helpers ───────────────────────────────────────────────────────

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value : null;
}

function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function asBoolean(value: unknown): boolean {
  return value === true;
}

function parseAudioTrackMetadata(audioStreams: unknown): AudioTrackItem[] {
  if (!Array.isArray(audioStreams)) return [];

  const items = audioStreams
    .map((stream, index): AudioTrackItem | null => {
      if (!isRecord(stream)) return null;

      // Support both normalized (codec/language/title) and raw ffprobe
      // (codec_name/tags.language/tags.title) field names.
      const tags = isRecord(stream.tags) ? stream.tags : {};
      const disposition = isRecord(stream.disposition)
        ? stream.disposition
        : {};
      const language =
        asString(stream.language) ?? asString(tags.language) ?? "";
      const title = asString(stream.title) ?? asString(tags.title);
      const codec = asString(stream.codec) ?? asString(stream.codec_name);
      const channels = asNumber(stream.channels);
      const bitrate =
        asNumber(stream.bitrate) ??
        (typeof stream.bit_rate === "string"
          ? Number(stream.bit_rate) || null
          : asNumber(stream.bit_rate));
      const channelSuffix =
        channels == null
          ? ""
          : channels === 1
            ? " Mono"
            : channels === 2
              ? " Stereo"
              : ` ${channels}ch`;

      return {
        id: index,
        label:
          title ??
          ([
            language && language !== "und" ? language.toUpperCase() : null,
            codec?.toUpperCase(),
          ]
            .filter((part): part is string => Boolean(part))
            .join(" ") ||
            `音轨 ${index + 1}${channelSuffix}`),
        language,
        title: title ?? undefined,
        codec: codec ?? undefined,
        channels,
        bitrate,
        selected: asBoolean(stream.isDefault) || disposition.default === 1,
        available: false,
      };
    })
    .filter((track): track is AudioTrackItem => track !== null);

  return normalizeAudioTrackSelection(items);
}

function normalizeAudioTrackSelection(
  tracks: AudioTrackItem[],
): AudioTrackItem[] {
  if (tracks.length === 0) return tracks;
  const selectedTrack = tracks.find((track) => track.selected) ?? tracks[0];
  return tracks.map((track) => ({
    ...track,
    selected: track.id === selectedTrack.id,
  }));
}

function areAudioTracksEqual(
  prev: AudioTrackItem[],
  next: AudioTrackItem[],
): boolean {
  return (
    prev.length === next.length &&
    prev.every((track, index) => {
      const nextTrack = next[index];
      return (
        nextTrack !== undefined &&
        track.id === nextTrack.id &&
        track.label === nextTrack.label &&
        track.language === nextTrack.language &&
        track.title === nextTrack.title &&
        track.codec === nextTrack.codec &&
        track.channels === nextTrack.channels &&
        track.bitrate === nextTrack.bitrate &&
        track.selected === nextTrack.selected &&
        track.available === nextTrack.available
      );
    })
  );
}

function areNativeTextTracksEqual(
  prev: NativeTextTrackInfo[],
  next: NativeTextTrackInfo[],
): boolean {
  return (
    prev.length === next.length &&
    prev.every((track, index) => {
      const nextTrack = next[index];
      return (
        nextTrack !== undefined &&
        track.index === nextTrack.index &&
        track.label === nextTrack.label &&
        track.language === nextTrack.language &&
        track.kind === nextTrack.kind
      );
    })
  );
}

/**
 * Determine the selected audio track's codec from audioStreams metadata.
 * Returns the codec string (e.g. "ac3", "eac3", "aac") or null.
 */
function getSelectedAudioCodec(
  audioStreams: unknown,
  selectedTrackId: number | null,
): string | null {
  if (!Array.isArray(audioStreams) || audioStreams.length === 0) return null;
  const idx = selectedTrackId ?? 0;
  const stream = audioStreams[idx >= 0 && idx < audioStreams.length ? idx : 0];
  if (!isRecord(stream)) return null;
  return asString(stream.codec) ?? asString(stream.codec_name);
}

// ── Hook params / return ──────────────────────────────────────────────────────

export interface UseVideoEngineParams {
  videoRef: RefObject<HTMLVideoElement | null>;
  streamUrl: string | null;
  filename: string;
  itemFileId: string | null;
  itemDuration: number | null;
  itemAudioStreams: unknown;
  itemResumePosition: number;
  isPlaying: boolean;
  /** Setters for subtitle native track sync. */
  setNativeTextTracks: Dispatch<SetStateAction<NativeTextTrackInfo[]>>;
  /** Playback state setters — engine resets these on stream change. */
  setStarted: Dispatch<SetStateAction<boolean>>;
  setDuration: Dispatch<SetStateAction<number>>;
  setCurrentTime: Dispatch<SetStateAction<number>>;
  setBufferedRanges: Dispatch<SetStateAction<{ start: number; end: number }[]>>;
  setWaiting: Dispatch<SetStateAction<boolean>>;
  pendingSeekTimeRef: MutableRefObject<number | null>;
  /** PlayerContext callbacks. */
  setPosition: (pos: number) => void;
  changeStreamAudioTrack: (idx: number) => Promise<void>;
}

export interface VideoEngineResult {
  hlsRef: RefObject<Hls | null>;
  audioTracks: AudioTrackItem[];
  changeAudioTrack: (idx: number) => void;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useVideoEngine({
  videoRef,
  streamUrl,
  filename,
  itemFileId,
  itemDuration,
  itemAudioStreams,
  itemResumePosition,
  isPlaying,
  setNativeTextTracks,
  setStarted,
  setDuration,
  setCurrentTime,
  setBufferedRanges,
  setWaiting,
  pendingSeekTimeRef,
  setPosition,
  changeStreamAudioTrack,
}: UseVideoEngineParams): VideoEngineResult {
  // ── Engine refs ──
  const hlsRef = useRef<Hls | null>(null);
  const flvRef = useRef<FlvPlayer | null>(null);
  const ac3PlayerRef = useRef<Ac3AudioPlayer | null>(null);
  const prevFileIdRef = useRef<string | null>(null);
  const lastResumeFileIdRef = useRef<string | null>(null);

  // Ref tracking isPlaying so engine-init closures can read the latest value
  // without adding isPlaying to deps (would destroy HLS on every toggle).
  const isPlayingRef = useRef(isPlaying);
  isPlayingRef.current = isPlaying;

  // Ref for engine init: avoids adding itemResumePosition to deps.
  const itemResumePositionRef = useRef(itemResumePosition);
  itemResumePositionRef.current = itemResumePosition;

  // ── Audio track state (engine-owned) ──
  const [detectedAudioTracks, setDetectedAudioTracks] = useState<
    AudioTrackItem[]
  >([]);
  const [selectedAudioTrackId, setSelectedAudioTrackId] = useState<
    number | null
  >(null);
  const pendingAudioTrackIdRef = useRef<number | null>(null);

  // ── Derived audio track list ──
  const audioTracks = useMemo<AudioTrackItem[]>(() => {
    const baseTracks =
      detectedAudioTracks.length > 0
        ? detectedAudioTracks
        : parseAudioTrackMetadata(itemAudioStreams);

    if (selectedAudioTrackId == null) return baseTracks;

    return normalizeAudioTrackSelection(
      baseTracks.map((track) => ({
        ...track,
        selected: track.id === selectedAudioTrackId,
      })),
    );
  }, [detectedAudioTracks, itemAudioStreams, selectedAudioTrackId]);

  // ── Audio preference loading ──
  useEffect(() => {
    const metadataTracks = parseAudioTrackMetadata(itemAudioStreams);
    const savedIndex = itemFileId
      ? loadAudioSelectionPreference(itemFileId)
      : null;
    const savedTrack =
      savedIndex != null
        ? metadataTracks.find((track) => track.id === savedIndex)
        : null;
    const defaultTrack =
      savedTrack ?? metadataTracks.find((track) => track.selected) ?? null;
    const nextTrackId = defaultTrack?.id ?? null;
    pendingAudioTrackIdRef.current = nextTrackId;
    setSelectedAudioTrackId(nextTrackId);
  }, [itemAudioStreams, itemFileId]);

  // ── Resume position ──
  useEffect(() => {
    if (!itemFileId) {
      lastResumeFileIdRef.current = null;
      return;
    }
    if (itemResumePosition <= 10) return;
    if (lastResumeFileIdRef.current === itemFileId) return;
    const video = videoRef.current;
    if (!video) return;
    lastResumeFileIdRef.current = itemFileId;
    video.currentTime = itemResumePosition;
    setPosition(itemResumePosition);
  }, [itemFileId, itemResumePosition, setPosition, videoRef]);

  // ── Engine destroy ──
  const destroyEngines = useCallback(() => {
    if (hlsRef.current) {
      try {
        hlsRef.current.destroy();
      } catch (e) {
        console.error(e);
      }
      hlsRef.current = null;
    }
    if (flvRef.current) {
      flvRef.current.destroy();
      flvRef.current = null;
    }
    if (ac3PlayerRef.current) {
      ac3PlayerRef.current.dispose();
      ac3PlayerRef.current = null;
    }
    const video = videoRef.current;
    if (video?.src) {
      // Don't call video.pause() — it fires the "pause" DOM event which
      // globally sets isPlaying=false via useVideoEvents' onPause handler.
      // During video-switch transitions the OLD VideoPlayer is still mounted
      // for one render cycle; its cleanup must not corrupt global state.
      // removeAttribute("src") + load() is sufficient to release resources.
      video.removeAttribute("src");
      video.load();
    }
  }, [videoRef]);
  // ── Engine initialisation ──
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const isSeamlessSwitch =
      prevFileIdRef.current !== null && prevFileIdRef.current === itemFileId;
    // Detect when PlayerContext.item changed to a DIFFERENT file while this
    // component is still mounted (happens during video-switch transition:
    // old window's VideoPlayer re-renders with the new file before being
    // unmounted).  In this case, destroy the current engine but do NOT create
    // a new one — the NEW VideoPlayer (mounted in the new window) will handle
    // engine init for the new file.  Creating an engine here would:
    //   1. Connect to the new file's server-side HLS transcode session
    //   2. Download segments on a video element that's about to be destroyed
    //   3. Leave the HLS session in an inconsistent state for the real player
    const isStaleFileSwitch =
      prevFileIdRef.current !== null && prevFileIdRef.current !== itemFileId;
    prevFileIdRef.current = itemFileId;
    const oneShotCleanups: (() => void)[] = [];

    destroyEngines();

    // Allow the resume-position effect to fire on stream reload.
    lastResumeFileIdRef.current = null;

    if (isStaleFileSwitch) {
      // Old component got a new file — just clean up and bail out.
      // The new VideoPlayer instance will initialise the engine.
      return () => {
        destroyEngines();
        for (const fn of oneShotCleanups) fn();
      };
    }

    if (isSeamlessSwitch) {
      setWaiting(true);
      setBufferedRanges((prev) => (prev.length === 0 ? prev : []));
      pendingSeekTimeRef.current = null;
    } else {
      setDetectedAudioTracks((prev) => (prev.length === 0 ? prev : []));
      setSelectedAudioTrackId(null);
      setNativeTextTracks((prev) => (prev.length === 0 ? prev : []));
      setStarted(false);
      setDuration(itemDuration ?? 0);
      const resumePos = itemResumePositionRef.current;
      setCurrentTime(resumePos > 10 ? resumePos : 0);
      setBufferedRanges((prev) => (prev.length === 0 ? prev : []));
      pendingSeekTimeRef.current = null;
    }

    if (!streamUrl) {
      return;
    }

    const isHLS = streamUrl.includes("/playlist.m3u8");
    const isFLV = filename.toLowerCase().endsWith(".flv");

    // Classify the selected audio track's playback strategy.
    const selectedCodec = getSelectedAudioCodec(
      itemAudioStreams,
      pendingAudioTrackIdRef.current,
    );
    const audioMode =
      isHLS || isFLV ? ("native" as const) : classifyAudioTrack(selectedCodec);
    const needsMediabunny = audioMode === "mediabunny";

    // ── Detailed engine decision log ──
    const audioTracks = Array.isArray(itemAudioStreams)
      ? itemAudioStreams.map((s: Record<string, unknown>) => ({
          codec: s.codec_name ?? s.codec,
          channels: s.channels,
          channelLayout: s.channel_layout,
          sampleRate: s.sample_rate,
          bitRate: s.bit_rate,
          lang: (s.tags as Record<string, unknown>)?.language,
          title: (s.tags as Record<string, unknown>)?.title,
        }))
      : null;
    const selectedIdx = pendingAudioTrackIdRef.current ?? 0;

    console.groupCollapsed(
      `%c[VideoEngine]%c ${needsMediabunny ? "🔧 Mediabunny" : isHLS ? "📡 HLS" : isFLV ? "📦 FLV" : "▶️ DirectPlay"} %c${filename}`,
      "color:#f97316;font-weight:bold",
      "color:#3b82f6;font-weight:bold",
      "color:#888;font-weight:normal",
    );
    console.log("Stream URL:", streamUrl);
    console.log("Format detection:", { isHLS, isFLV });
    console.log("Audio tracks:", audioTracks);
    console.log(
      `Selected audio: track[${selectedIdx}] → codec="${selectedCodec}" → mode="${audioMode}"`,
    );
    console.groupEnd();

    if (isHLS) {
      if (Hls.isSupported()) {
        Hls.DefaultConfig.lowLatencyMode = false;
        Hls.DefaultConfig.backBufferLength = Number.POSITIVE_INFINITY;
        Hls.DefaultConfig.liveBackBufferLength = 90;

        const maxBufferLength = 30;
        const hlsStartPosition =
          itemResumePositionRef.current > 10
            ? itemResumePositionRef.current
            : -1;

        const hls = new Hls({
          enableWorker: true,
          manifestLoadingTimeOut: 20000,
          maxBufferLength,
          maxMaxBufferLength: maxBufferLength,
          videoPreference: { preferHDR: true },
          startPosition: hlsStartPosition,
          fragLoadPolicy: {
            default: {
              maxTimeToFirstByteMs: 60_000,
              maxLoadTimeMs: 120_000,
              timeoutRetry: {
                maxNumRetry: 4,
                retryDelayMs: 0,
                maxRetryDelayMs: 0,
              },
              errorRetry: {
                maxNumRetry: 6,
                retryDelayMs: 1000,
                maxRetryDelayMs: 8000,
              },
            },
          },
        });

        const isTranscodeSession = streamUrl.includes("/api/hls/");
        if (!isTranscodeSession) {
          hls.on(Hls.Events.AUDIO_TRACKS_UPDATED, (_, data) => {
            const nextTracks = normalizeAudioTrackSelection(
              data.audioTracks.map((t, i) => ({
                id: i,
                label: t.name || t.lang || `Track ${i + 1}`,
                language: t.lang ?? "",
                title: t.name ?? undefined,
                codec: undefined,
                channels: null,
                bitrate: null,
                selected: i === hls.audioTrack,
                available: true,
              })),
            );
            setDetectedAudioTracks((prev) =>
              areAudioTracksEqual(prev, nextTracks) ? prev : nextTracks,
            );
          });

          hls.on(Hls.Events.AUDIO_TRACK_SWITCHED, (_, data) => {
            setDetectedAudioTracks((prev) => {
              const nextTracks = normalizeAudioTrackSelection(
                prev.map((t) => ({ ...t, selected: t.id === data.id })),
              );
              return areAudioTracksEqual(prev, nextTracks) ? prev : nextTracks;
            });
          });
        }

        hls.on(Hls.Events.MANIFEST_PARSED, () => {
          if (!video.duration || !Number.isFinite(video.duration)) {
            if (itemDuration) setDuration(itemDuration);
          }
          if (isSeamlessSwitch && itemResumePositionRef.current > 10) {
            video.currentTime = itemResumePositionRef.current;
            setPosition(itemResumePositionRef.current);
            lastResumeFileIdRef.current = itemFileId;
            video.play().catch(() => {});
          } else {
            const resumePos = itemResumePositionRef.current;
            if (resumePos > 10) {
              lastResumeFileIdRef.current = itemFileId;
            }
            if (isPlayingRef.current) {
              video.play().catch(() => {});
            }
          }
        });

        // ── hls.js error recovery (matches Jellyfin htmlMediaHelper) ──
        let lastRecoverTime = 0;
        let lastSwapTime = 0;

        hls.on(Hls.Events.ERROR, (_event, data) => {
          console.error(
            `[HLS Error] type=${data.type} details=${data.details ?? ""} fatal=${data.fatal ?? false}`,
          );

          if (
            data.type === Hls.ErrorTypes.NETWORK_ERROR &&
            data.response &&
            typeof data.response === "object" &&
            "code" in data.response &&
            typeof data.response.code === "number" &&
            data.response.code >= 400 &&
            data.response.code !== 404
          ) {
            console.error("[HLS] server error, destroying");
            hls.destroy();
            return;
          }

          if (!data.fatal) return;

          const now = performance.now();

          switch (data.type) {
            case Hls.ErrorTypes.NETWORK_ERROR:
              if (
                data.response &&
                typeof data.response === "object" &&
                "code" in data.response &&
                data.response.code === 0
              ) {
                console.error("[HLS] CORS error (response code 0), destroying");
                hls.destroy();
              } else {
                console.debug("[HLS] fatal network error, calling startLoad()");
                hls.startLoad();
              }
              break;
            case Hls.ErrorTypes.MEDIA_ERROR:
              if (now - lastRecoverTime > 3000) {
                lastRecoverTime = now;
                console.debug("[HLS] recovering media error");
                hls.recoverMediaError();
              } else if (now - lastSwapTime > 3000) {
                lastSwapTime = now;
                console.debug("[HLS] swapping audio codec and recovering");
                hls.swapAudioCodec();
                hls.recoverMediaError();
              } else {
                console.error("[HLS] unrecoverable media error, destroying");
                hls.destroy();
              }
              break;
            default:
              console.error("[HLS] fatal error, destroying");
              hls.destroy();
              break;
          }
        });

        hls.loadSource(streamUrl);
        hls.attachMedia(video);
        hlsRef.current = hls;
      } else {
        // Safari native HLS
        video.src = streamUrl;
        video.load();
        if (isPlayingRef.current) {
          const onCanPlaySafari = () => video.play().catch(() => {});
          video.addEventListener("canplay", onCanPlaySafari, { once: true });
          oneShotCleanups.push(() =>
            video.removeEventListener("canplay", onCanPlaySafari),
          );
        }
      }
    } else if (isFLV && flvjs.isSupported()) {
      const flv = flvjs.createPlayer(
        { type: "flv", url: streamUrl },
        { enableWorker: false },
      );
      flv.attachMediaElement(video);
      flv.load();
      flvRef.current = flv;
      if (!isSeamlessSwitch && isPlayingRef.current) {
        const onCanPlayFlv = () => video.play().catch(() => {});
        video.addEventListener("canplay", onCanPlayFlv, { once: true });
        oneShotCleanups.push(() =>
          video.removeEventListener("canplay", onCanPlayFlv),
        );
      }
    } else if (needsMediabunny) {
      // ── Mediabunny AC3/EAC3 → Web Audio ──
      // Video: DirectPlay (`video.src`), browser handles all video I/O.
      // Audio: mediabunny decodes AC3/EAC3 → AudioBuffer → Web Audio API.
      // Following the official mediabunny media-player example pattern.

      // DirectPlay for video (browser handles seeking/buffering natively)
      video.src = streamUrl;
      video.load();

      if (itemDuration && itemDuration > 0) {
        setDuration(itemDuration);
      }

      const ac3Player = new Ac3AudioPlayer(video);
      ac3PlayerRef.current = ac3Player;

      // mediabunny reads from the full stream URL and demuxes the audio track
      // client-side.  This fetches more data than needed (video packets are
      // discarded), but works instantly with any source type (local/SMB/NFS/S3).
      // A server-side audio-only endpoint (/audio-stream) exists for local files
      // but is too slow for remote sources (must read the entire container).
      const trackIdx = pendingAudioTrackIdRef.current ?? 0;
      ac3Player.init(streamUrl, trackIdx);

      const onCanPlayAc3 = () => {
        const resumePos = itemResumePositionRef.current;
        if (resumePos > 10) {
          video.currentTime = resumePos;
          setPosition(resumePos);
          lastResumeFileIdRef.current = itemFileId;
        }
        // Start audio scheduling (instant if init() already finished,
        // deferred automatically if still probing).
        ac3Player.playFrom(video.currentTime);
        if (isPlayingRef.current) {
          video.play().catch(() => {});
        }
      };
      video.addEventListener("canplay", onCanPlayAc3, { once: true });
      oneShotCleanups.push(() =>
        video.removeEventListener("canplay", onCanPlayAc3),
      );

      console.log(
        "%c[Mediabunny]%c 🔊 AC3 Web Audio pipeline (DirectPlay for video)",
        "color:#f97316;font-weight:bold",
        "color:#3b82f6",
      );

      oneShotCleanups.push(() => {
        ac3Player.dispose();
        if (ac3PlayerRef.current === ac3Player) {
          ac3PlayerRef.current = null;
        }
      });
    } else {
      // DirectPlay
      video.src = streamUrl;
      video.load();
      if (isSeamlessSwitch && itemResumePositionRef.current > 10) {
        const onLoadedForSeek = () => {
          video.currentTime = itemResumePositionRef.current;
          setPosition(itemResumePositionRef.current);
          lastResumeFileIdRef.current = itemFileId;
          video.play().catch(() => {});
        };
        video.addEventListener("loadedmetadata", onLoadedForSeek, {
          once: true,
        });
        oneShotCleanups.push(() =>
          video.removeEventListener("loadedmetadata", onLoadedForSeek),
        );
      } else if (isPlayingRef.current) {
        const onCanPlayNative = () => video.play().catch(() => {});
        video.addEventListener("canplay", onCanPlayNative, { once: true });
        oneShotCleanups.push(() =>
          video.removeEventListener("canplay", onCanPlayNative),
        );
      }
    }

    // Native audio & text track detection for non-HLS engines
    const onNativeAudioTracks = () => {
      const nativeTracks = video.audioTracks;
      if (!nativeTracks || nativeTracks.length === 0) return;
      const items: AudioTrackItem[] = [];
      for (let i = 0; i < nativeTracks.length; i++) {
        const t = nativeTracks[i];
        items.push({
          id: i,
          label: t.label || t.language || `Track ${i + 1}`,
          language: t.language ?? "",
          selected: t.enabled,
          available: true,
        });
      }
      const pendingTrackId = pendingAudioTrackIdRef.current;
      if (pendingTrackId != null && nativeTracks[pendingTrackId]) {
        // Only set native track selection when NOT using mediabunny
        // (mediabunny silences native audio via createMediaElementSource).
        if (!needsMediabunny) {
          for (let i = 0; i < nativeTracks.length; i++) {
            nativeTracks[i].enabled = i === pendingTrackId;
          }
        }
      }
      const nextTracks = normalizeAudioTrackSelection(items);
      setDetectedAudioTracks((prev) =>
        areAudioTracksEqual(prev, nextTracks) ? prev : nextTracks,
      );
    };
    const syncNativeTextTracks = () => {
      const items: NativeTextTrackInfo[] = [];
      for (let i = 0; i < video.textTracks.length; i++) {
        const track = video.textTracks[i];
        if (track.kind !== "subtitles" && track.kind !== "captions") continue;
        items.push({
          index: i,
          label: track.label ?? "",
          language: track.language ?? "",
          kind: track.kind,
        });
      }
      // Only update state when content actually changed — prevents a feedback
      // loop where addTextTrack() → canplay → setNativeTextTracks(new ref) →
      // subtitleTracks recalc → subtitle effect re-runs → repeat.
      setNativeTextTracks((prev) =>
        areNativeTextTracksEqual(prev, items) ? prev : items,
      );
    };
    video.addEventListener("loadedmetadata", onNativeAudioTracks);
    video.addEventListener("loadedmetadata", syncNativeTextTracks);
    video.addEventListener("loadeddata", syncNativeTextTracks);
    video.addEventListener("canplay", syncNativeTextTracks);

    return () => {
      for (const cleanup of oneShotCleanups) cleanup();
      video.removeEventListener("loadedmetadata", onNativeAudioTracks);
      video.removeEventListener("loadedmetadata", syncNativeTextTracks);
      video.removeEventListener("loadeddata", syncNativeTextTracks);
      video.removeEventListener("canplay", syncNativeTextTracks);
      destroyEngines();
    };
  }, [
    streamUrl,
    filename,
    itemDuration,
    itemAudioStreams,
    destroyEngines,
    itemFileId,
    setPosition,
    pendingSeekTimeRef,
    setBufferedRanges,
    setCurrentTime,
    setDuration,
    setNativeTextTracks,
    setStarted,
    setWaiting,
    videoRef,
  ]);

  // ── Audio track switching ──
  const changeAudioTrack = useCallback(
    (idx: number) => {
      pendingAudioTrackIdRef.current = idx;
      setSelectedAudioTrackId(idx);

      if (itemFileId) {
        saveAudioSelectionPreference(itemFileId, idx);
      }

      // hls.js native multi-audio
      if (hlsRef.current && hlsRef.current.audioTracks.length > 1) {
        hlsRef.current.audioTrack = idx;
        return;
      }

      // HLS transcode session: re-resolve stream with new audio
      const isHlsTranscode = streamUrl?.includes("/playlist.m3u8");
      if (isHlsTranscode) {
        changeStreamAudioTrack(idx);
        return;
      }

      // ── DirectPlay track switching (three modes) ──
      const video = videoRef.current;
      if (!video || !streamUrl) return;

      const newCodec = getSelectedAudioCodec(itemAudioStreams, idx);
      const newMode = classifyAudioTrack(newCodec);
      const currentPosition = video.currentTime;

      // Always dispose existing Ac3AudioPlayer first (restores native audio).
      if (ac3PlayerRef.current) {
        ac3PlayerRef.current.dispose();
        ac3PlayerRef.current = null;
      }

      switch (newMode) {
        case "mediabunny": {
          // AC3/EAC3 → WASM decode → Web Audio. Constructor silences native audio.
          const ac3Player = new Ac3AudioPlayer(video);
          ac3PlayerRef.current = ac3Player;
          ac3Player.init(streamUrl, idx).then((ready) => {
            if (ready && ac3PlayerRef.current === ac3Player) {
              ac3Player.playFrom(video.currentTime);
            }
          });
          console.log(
            `%c[VideoEngine]%c 🔄 → Mediabunny track[${idx}] (${newCodec})`,
            "color:#f97316;font-weight:bold",
            "color:#22c55e",
          );
          break;
        }

        case "native": {
          // Browser can decode natively — just switch the audioTrack.
          // dispose() above already restored native audio via unmuteNativeAudio.
          const nativeTracks = video.audioTracks;
          if (nativeTracks) {
            for (let i = 0; i < nativeTracks.length; i++) {
              nativeTracks[i].enabled = i === idx;
            }
          }
          console.log(
            `%c[VideoEngine]%c 🔄 → Native track[${idx}] (${newCodec})`,
            "color:#f97316;font-weight:bold",
            "color:#3b82f6",
          );
          break;
        }

        case "server-transcode": {
          // Unsupported codec — need server FFmpeg. Re-resolve stream URL.
          console.log(
            `%c[VideoEngine]%c 🔄 → Server transcode track[${idx}] (${newCodec})`,
            "color:#f97316;font-weight:bold",
            "color:#ef4444",
          );
          changeStreamAudioTrack(idx);
          break;
        }
      }

      // For non-server-transcode modes, seek back to maintain position.
      if (newMode !== "server-transcode") {
        video.currentTime = currentPosition;
      }

      setDetectedAudioTracks((prev) => {
        const nextTracks = normalizeAudioTrackSelection(
          prev.map((t) => ({ ...t, selected: t.id === idx })),
        );
        return areAudioTracksEqual(prev, nextTracks) ? prev : nextTracks;
      });
    },
    [streamUrl, changeStreamAudioTrack, itemFileId, itemAudioStreams, videoRef],
  );

  return { hlsRef, audioTracks, changeAudioTrack };
}
