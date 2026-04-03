/**
 * useSubtitleManager — subtitle lifecycle management for VideoPlayer.
 *
 * Owns all subtitle-related state, refs, and effects:
 *   - Track list derivation (native + runtime + item subtitles)
 *   - Subtitle renderer init/destroy (native VTT, ASS/libass-wasm, PGS/libpgs, SSE)
 *   - Style persistence & cue layout
 *   - Seek refresh for SSE subtitles
 */
import { PgsRenderer } from "libpgs";
import type { Dispatch, RefObject, SetStateAction } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  decodeSubtitleBytes,
  getSubtitleCueLine,
  looksLikeAssSubtitleContent,
  normalizeSubtitleUrl,
  pickPreferredSubtitleId,
  type SubtitleStyleSettings,
  saveSubtitleSelectionPreference,
  saveSubtitleStyleSettings,
  toSanitizedVtt,
  toSanitizedVttFromAss,
  toSanitizedVttFromSrt,
} from "@/lib/player-subtitles";
import type { SubtitleTrackItem } from "@/system/media/VideoStateContext";
import type { SubtitleOutput } from "@/types";
import { PgsCanvasRenderer } from "./PgsCanvasRenderer";
import { SubtitleStreamLoader } from "./SubtitleStreamLoader";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface NativeTextTrackInfo {
  index: number;
  label: string;
  language: string;
  kind: string;
}

interface SubtitlesOctopusInstance {
  dispose(): void;
}

type SubtitlesOctopusCtor = new (
  opts: Record<string, unknown>,
) => SubtitlesOctopusInstance;

// ── Format constants & helpers ────────────────────────────────────────────────

const TEXT_FORMATS = new Set(["srt", "vtt", "subrip", "webvtt"]);
const ASS_FORMATS = new Set(["ass", "ssa"]);
const PGS_FORMATS = new Set(["pgs", "sup", "hdmv_pgs_subtitle"]);

function isTextFmt(fmt: string) {
  return TEXT_FORMATS.has(fmt.toLowerCase());
}
function isAssFmt(fmt: string) {
  return ASS_FORMATS.has(fmt.toLowerCase());
}
function isPgsFmt(fmt: string) {
  return PGS_FORMATS.has(fmt.toLowerCase());
}

function normalizeSubtitleFormat(format: string | null | undefined): string {
  const normalized = format?.trim().toLowerCase() ?? "srt";
  switch (normalized) {
    case "subrip":
    case "mov_text":
    case "hdmv_text_subtitle":
    case "text":
      return "srt";
    case "webvtt":
      return "vtt";
    case "ssa":
      return "ass";
    case "hdmv_pgs_subtitle":
      return "pgs";
    default:
      return normalized || "srt";
  }
}

function normalizeTrackText(text: string): string {
  return text.trim().toLowerCase();
}

function normalizeTrackLanguage(language: string): string {
  return language.trim().toLowerCase().replaceAll("_", "-");
}

function usesEmbeddedSubtitleStream(track: SubtitleTrackItem): boolean {
  return (
    track.sourceType === "embedded" &&
    (isTextFmt(track.format) ||
      isAssFmt(track.format) ||
      isPgsFmt(track.format))
  );
}

function areSubtitleTracksEqual(
  prev: SubtitleTrackItem[],
  next: SubtitleTrackItem[],
): boolean {
  return (
    prev.length === next.length &&
    prev.every((track, index) => {
      const nextTrack = next[index];
      if (!nextTrack) {
        return false;
      }

      const prevNativeTrackIndex = usesEmbeddedSubtitleStream(track)
        ? undefined
        : track.nativeTrackIndex;
      const nextNativeTrackIndex = usesEmbeddedSubtitleStream(nextTrack)
        ? undefined
        : nextTrack.nativeTrackIndex;

      return (
        track.id === nextTrack.id &&
        track.label === nextTrack.label &&
        track.language === nextTrack.language &&
        track.format === nextTrack.format &&
        track.storageUrl === nextTrack.storageUrl &&
        track.sourceType === nextTrack.sourceType &&
        track.isDefault === nextTrack.isDefault &&
        track.available === nextTrack.available &&
        prevNativeTrackIndex === nextNativeTrackIndex
      );
    })
  );
}

function bindNativeTextTracks(
  tracks: SubtitleTrackItem[],
  nativeTracks: NativeTextTrackInfo[],
): SubtitleTrackItem[] {
  const candidates = nativeTracks
    .filter(
      (track) =>
        track.kind === "subtitles" ||
        track.kind === "captions" ||
        track.kind === "",
    )
    .map((track, position) => ({ ...track, position }));
  const usedPositions = new Set<number>();

  return tracks.map((track) => {
    if (track.sourceType !== "embedded") {
      return {
        ...track,
        available: Boolean(track.storageUrl),
      };
    }

    const trackLanguage = normalizeTrackLanguage(track.language);
    const trackLabel = normalizeTrackText(track.label);
    const exactMatch = candidates.find(
      (candidate) =>
        !usedPositions.has(candidate.position) &&
        trackLanguage.length > 0 &&
        normalizeTrackLanguage(candidate.language) === trackLanguage &&
        trackLabel.length > 0 &&
        normalizeTrackText(candidate.label) === trackLabel,
    );
    const languageMatch =
      exactMatch ??
      candidates.find(
        (candidate) =>
          !usedPositions.has(candidate.position) &&
          trackLanguage.length > 0 &&
          normalizeTrackLanguage(candidate.language) === trackLanguage,
      );
    const fallbackMatch =
      languageMatch ??
      candidates.find((candidate) => !usedPositions.has(candidate.position));

    if (!fallbackMatch) {
      return {
        ...track,
        available:
          isTextFmt(track.format) ||
          isAssFmt(track.format) ||
          isPgsFmt(track.format)
            ? true
            : Boolean(track.storageUrl),
      };
    }

    usedPositions.add(fallbackMatch.position);
    return {
      ...track,
      label: track.label || fallbackMatch.label || track.language,
      nativeTrackIndex: fallbackMatch.index,
      available: true,
    };
  });
}

function isVttCue(cue: TextTrackCue): cue is VTTCue {
  return "line" in cue && "snapToLines" in cue;
}

function applyCueLayout(
  track: TextTrack,
  settings: SubtitleStyleSettings,
): void {
  if (!track.cues) return;
  for (let index = 0; index < track.cues.length; index++) {
    const cue = track.cues[index];
    if (!cue || !isVttCue(cue)) continue;
    cue.snapToLines = false;
    cue.line = getSubtitleCueLine(settings.position);
    cue.position = 50;
    cue.size = 90;
    cue.align = "center";
  }
}

async function createTextTrackObjectUrl(
  storageUrl: string,
  format: string,
): Promise<string> {
  const response = await fetch(storageUrl, { credentials: "include" });
  if (!response.ok) {
    throw new Error(`subtitle ${response.status}`);
  }

  const subtitleContent = decodeSubtitleBytes(await response.arrayBuffer());
  const normalizedFormat = format.toLowerCase();
  const vttContent =
    normalizedFormat === "srt"
      ? toSanitizedVttFromSrt(subtitleContent)
      : looksLikeAssSubtitleContent(subtitleContent)
        ? toSanitizedVttFromAss(subtitleContent)
        : toSanitizedVtt(subtitleContent);

  return URL.createObjectURL(
    new Blob([vttContent], { type: "text/vtt;charset=utf-8" }),
  );
}

// ── Hook params / return ──────────────────────────────────────────────────────

export interface UseSubtitleManagerParams {
  videoRef: RefObject<HTMLVideoElement | null>;
  streamUrl: string | null;
  itemFileId: string | null;
  itemSubtitles: SubtitleOutput[];
  initialSubtitleSettings: SubtitleStyleSettings;
}

export interface SubtitleManagerResult {
  subtitleTracks: SubtitleTrackItem[];
  activeSubtitleId: string | null;
  subtitleStyleSettings: SubtitleStyleSettings;
  setSubtitle: (id: string | null) => void;
  updateSubtitleStyleSettings: (next: Partial<SubtitleStyleSettings>) => void;
  registerSubtitleTrack: (track: SubtitleTrackItem) => void;
  removeSubtitleTrack: (id: string) => void;
  /** Exposed for useVideoEngine to call when native text tracks change. */
  setNativeTextTracks: Dispatch<SetStateAction<NativeTextTrackInfo[]>>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useSubtitleManager({
  videoRef,
  streamUrl,
  itemFileId,
  itemSubtitles,
  initialSubtitleSettings,
}: UseSubtitleManagerParams): SubtitleManagerResult {
  // ── State ──
  const [activeSubtitleId, setActiveSubtitleId] = useState<string | null>(null);
  const [subtitleStyleSettings, setSubtitleStyleSettings] =
    useState<SubtitleStyleSettings>(() => initialSubtitleSettings);
  const [runtimeSubtitleTracks, setRuntimeSubtitleTracks] = useState<
    SubtitleTrackItem[]
  >([]);
  const [removedSubtitleIds, setRemovedSubtitleIds] = useState<string[]>([]);
  const [nativeTextTracks, setNativeTextTracks] = useState<
    NativeTextTrackInfo[]
  >([]);
  const [assFailedSubtitleId, setAssFailedSubtitleId] = useState<string | null>(
    null,
  );

  // ── Refs ──
  const assRef = useRef<SubtitlesOctopusInstance | null>(null);
  const pgsRef = useRef<PgsRenderer | null>(null);
  const pgsCanvasRendererRef = useRef<PgsCanvasRenderer | null>(null);
  const subtitleStreamLoaderRef = useRef<SubtitleStreamLoader | null>(null);
  const subtitleObjectUrlsRef = useRef<string[]>([]);
  const subtitleStyleSettingsRef = useRef<SubtitleStyleSettings>(
    initialSubtitleSettings,
  );
  const stableSubtitleTracksRef = useRef<SubtitleTrackItem[]>([]);
  // Always-current ref for subtitleTracks — used by the subtitle setup effect
  // to read the latest tracks without including the full array in deps.
  const subtitleTracksRef = useRef<SubtitleTrackItem[]>([]);

  // ── Derived track list ──
  const subtitleTracks = useMemo<SubtitleTrackItem[]>(() => {
    const mergedTracks = [...itemSubtitles, ...runtimeSubtitleTracks]
      .map((subtitle) => ({
        id: subtitle.id,
        label:
          ("label" in subtitle ? subtitle.label : subtitle.title) ||
          subtitle.language,
        language: subtitle.language,
        format: normalizeSubtitleFormat(subtitle.format),
        storageUrl: normalizeSubtitleUrl(subtitle.storageUrl ?? null),
        sourceType: subtitle.sourceType,
        isDefault: subtitle.isDefault,
        available:
          subtitle.sourceType === "embedded"
            ? isTextFmt(normalizeSubtitleFormat(subtitle.format)) ||
              isAssFmt(normalizeSubtitleFormat(subtitle.format)) ||
              isPgsFmt(normalizeSubtitleFormat(subtitle.format))
            : Boolean(subtitle.storageUrl),
      }))
      .filter((track) => !removedSubtitleIds.includes(track.id))
      .filter(
        (track, index, tracks) =>
          tracks.findIndex((itemTrack) => itemTrack.id === track.id) === index,
      );

    const nextTracks = bindNativeTextTracks(mergedTracks, nativeTextTracks);
    const prevTracks = stableSubtitleTracksRef.current;
    if (areSubtitleTracksEqual(prevTracks, nextTracks)) {
      return prevTracks;
    }
    stableSubtitleTracksRef.current = nextTracks;
    return nextTracks;
  }, [
    itemSubtitles,
    runtimeSubtitleTracks,
    removedSubtitleIds,
    nativeTextTracks,
  ]);
  subtitleTracksRef.current = subtitleTracks;

  // Stable identity key for the active subtitle — used as the dependency for
  // the subtitle setup effect instead of the full `subtitleTracks` array.
  // For SSE subtitles, `nativeTrackIndex` is excluded because SSE subtitles
  // manage their own TextTrack and don't use the native track path; including
  // it would create a feedback loop (SSE creates track → nativeTrackIndex
  // flips → effect re-fires → destroys/recreates SSE loader → repeat).
  const activeSubStableKey = useMemo(() => {
    if (!activeSubtitleId) return null;
    const sub = subtitleTracks.find((t) => t.id === activeSubtitleId);
    if (!sub) return null;
    const fmt = normalizeSubtitleFormat(sub.format);
    const isSSE =
      sub.sourceType === "embedded" &&
      (isTextFmt(fmt) || isAssFmt(fmt) || isPgsFmt(fmt));
    // For SSE subtitles, use a fixed sentinel so nativeTrackIndex changes
    // don't cause the effect to re-run.
    const nativeIdx = isSSE ? "sse" : String(sub.nativeTrackIndex ?? "none");
    return `${sub.id}|${sub.format}|${sub.sourceType}|${sub.storageUrl ?? ""}|${nativeIdx}`;
  }, [activeSubtitleId, subtitleTracks]);

  // ── Style persistence ──
  useEffect(() => {
    subtitleStyleSettingsRef.current = subtitleStyleSettings;
    saveSubtitleStyleSettings(subtitleStyleSettings);
  }, [subtitleStyleSettings]);

  // ── Renderer lifecycle ──
  const destroySubtitleRenderers = useCallback(() => {
    if (assRef.current) {
      try {
        assRef.current.dispose();
      } catch (error) {
        console.warn("[VideoPlayer] failed to dispose ASS renderer", error);
      }
      assRef.current = null;
    }
    pgsRef.current?.dispose();
    pgsRef.current = null;
    pgsCanvasRendererRef.current?.dispose();
    pgsCanvasRendererRef.current = null;
    subtitleStreamLoaderRef.current?.destroy();
    subtitleStreamLoaderRef.current = null;
    const video = videoRef.current;
    if (video) {
      for (let i = 0; i < video.textTracks.length; i++) {
        video.textTracks[i].mode = "disabled";
      }
    }
    video?.querySelectorAll("track").forEach((t) => {
      t.remove();
    });
    video?.parentElement
      ?.querySelectorAll('canvas[data-pgs-overlay="true"]')
      .forEach((c) => {
        c.remove();
      });
    subtitleObjectUrlsRef.current.forEach((url) => {
      URL.revokeObjectURL(url);
    });
    subtitleObjectUrlsRef.current = [];
  }, [videoRef]);

  // ── Subtitle setup effect ──
  // Uses `activeSubStableKey` instead of `subtitleTracks` in deps to avoid
  // re-firing when only `nativeTrackIndex` changes for SSE subtitles.
  // Reads the latest tracks from `subtitleTracksRef` at execution time.
  // biome-ignore lint/correctness/useExhaustiveDependencies: activeSubStableKey is an intentional proxy dep replacing subtitleTracks to prevent feedback loops
  useEffect(() => {
    const video = videoRef.current;
    if (!activeSubtitleId || !video) {
      destroySubtitleRenderers();
      return;
    }

    const sub = subtitleTracksRef.current.find(
      (track) => track.id === activeSubtitleId,
    );
    if (!sub) {
      destroySubtitleRenderers();
      return;
    }

    const storageUrl = sub.storageUrl;
    const fmt = sub.format ?? "srt";
    const renderMode = subtitleStyleSettings.renderMode;
    const forceNativeAss = renderMode === "native" && isAssFmt(fmt);
    const useCustomOverlay = renderMode === "custom";
    const trackDisplayMode: TextTrackMode = useCustomOverlay
      ? "hidden"
      : "showing";

    const streamAccessToken =
      streamUrl?.match(/[?&]accessToken=([^&]+)/)?.[1] ?? "";

    const useSSE =
      sub.sourceType === "embedded" &&
      (isTextFmt(fmt) || isAssFmt(fmt) || isPgsFmt(fmt));

    // ── Native track path ──
    if (sub.nativeTrackIndex != null && !useSSE) {
      destroySubtitleRenderers();
      const nativeTrack = video.textTracks[sub.nativeTrackIndex];
      if (nativeTrack) {
        applyCueLayout(nativeTrack, subtitleStyleSettingsRef.current);
        nativeTrack.mode = trackDisplayMode;
      }
      return;
    }

    if (!useSSE && !storageUrl) {
      destroySubtitleRenderers();
      return;
    }
    let disposed = false;

    const loadTextTrack = async (trackFormat: string) => {
      const trackEl = document.createElement("track");
      trackEl.kind = "subtitles";
      trackEl.label = sub.label || sub.language;
      trackEl.srclang = sub.language;
      trackEl.default = true;
      const objectUrl = await createTextTrackObjectUrl(
        storageUrl ?? "",
        trackFormat,
      );
      subtitleObjectUrlsRef.current.push(objectUrl);
      trackEl.src = objectUrl;

      if (disposed) return;

      video.appendChild(trackEl);

      trackEl.addEventListener(
        "load",
        () => {
          if (disposed) return;
          const nativeTrack = trackEl.track;
          if (nativeTrack) {
            applyCueLayout(nativeTrack, subtitleStyleSettingsRef.current);
            nativeTrack.mode = trackDisplayMode;
          }
        },
        { once: true },
      );
    };

    // ── SSE path (embedded subtitles) ──
    if (useSSE) {
      const existing = subtitleStreamLoaderRef.current;
      if (existing && existing.subtitleId === sub.id && !existing.isDestroyed) {
        if (streamUrl) existing.ensureConnected();
        existing.applyLayout(subtitleStyleSettingsRef.current);
        const existingTrack = existing.getTrack();
        if (existingTrack) {
          existingTrack.mode = trackDisplayMode;
        }
        return () => {
          disposed = true;
        };
      }

      destroySubtitleRenderers();
      const accessToken = streamAccessToken;
      const loader = new SubtitleStreamLoader(
        video,
        sub.id,
        sub.language,
        sub.label || sub.language,
        accessToken,
        fmt,
      );
      subtitleStreamLoaderRef.current = loader;
      if (streamUrl) loader.ensureConnected();

      if (isPgsFmt(fmt) && videoRef.current?.parentElement) {
        const pgsCanvas = new PgsCanvasRenderer(
          video,
          videoRef.current.parentElement,
        );
        pgsCanvasRendererRef.current = pgsCanvas;
        loader.setPgsRenderer(pgsCanvas);
      }

      const loaderTrack = loader.getTrack();
      if (loaderTrack) {
        loader.applyLayout(subtitleStyleSettingsRef.current);
      }
      return () => {
        disposed = true;
      };
    }

    // ── Non-SSE path (external subtitle via storageUrl) ──
    destroySubtitleRenderers();

    if (isTextFmt(fmt)) {
      void loadTextTrack(fmt).catch((error: unknown) => {
        console.error("[VideoPlayer] failed to load text subtitle", error);
      });
    } else if (isAssFmt(fmt) && forceNativeAss) {
      void loadTextTrack(fmt).catch((error: unknown) => {
        console.error(
          "[VideoPlayer] failed to load ASS subtitle in native mode",
          error,
        );
      });
    } else if (isAssFmt(fmt) && assFailedSubtitleId === sub.id) {
      console.warn(
        "[VideoPlayer] ASS renderer previously failed, using VTT fallback",
      );
      void loadTextTrack("ass").catch((error: unknown) => {
        console.error(
          "[VideoPlayer] failed to load ASS fallback subtitle",
          error,
        );
      });
    } else if (isAssFmt(fmt)) {
      import("@jellyfin/libass-wasm")
        .then((mod) => {
          if (disposed) return;

          const SubtitlesOctopus = mod.default as SubtitlesOctopusCtor;
          assRef.current = new SubtitlesOctopus({
            video,
            subUrl: storageUrl,
            workerUrl: "/libass/subtitles-octopus-worker.js",
            legacyWorkerUrl: "/libass/subtitles-octopus-worker-legacy.js",
            fallbackFont:
              "/fonts/noto-sans-sc-chinese-simplified-400-normal.woff2",
            fonts: ["/libass/default.woff2"],
            availableFonts: {},
            libassMemoryLimit: 40,
            libassGlyphLimit: 40,
            targetFps: 24,
            prescaleTradeoff: 0.5,
            onDemandRender: true,
            blendMode: "js",
            onError: () => {
              console.error("[VideoPlayer] ASS renderer failed, falling back");
              if (disposed) return;
              assRef.current = null;
              setAssFailedSubtitleId(sub.id);
            },
          });
        })
        .catch((error: unknown) => {
          console.error(
            "[VideoPlayer] failed to initialize ASS renderer",
            error,
          );
          if (disposed) return;
          setAssFailedSubtitleId(sub.id);
        });
    } else if (isPgsFmt(fmt)) {
      try {
        const renderer = new PgsRenderer({
          video,
          workerUrl: "/libpgs/libpgs.worker.js",
          subUrl: storageUrl ?? undefined,
          aspectRatio: "contain",
        });
        pgsRef.current = renderer;
        renderer.renderAtTimestamp(video.currentTime);
      } catch {
        // libpgs init failed — fall back silently
      }
    }

    return () => {
      disposed = true;
      destroySubtitleRenderers();
    };
  }, [
    activeSubtitleId,
    assFailedSubtitleId,
    activeSubStableKey,
    destroySubtitleRenderers,
    streamUrl,
    subtitleStyleSettings.renderMode,
    videoRef,
  ]);

  // Destroy subtitle renderers on unmount
  useEffect(() => {
    return () => {
      destroySubtitleRenderers();
    };
  }, [destroySubtitleRenderers]);

  // Refresh SSE subtitle cues after seek
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    const onSeeked = () => {
      subtitleStreamLoaderRef.current?.onSeek(video.currentTime);
    };
    video.addEventListener("seeked", onSeeked);
    return () => video.removeEventListener("seeked", onSeeked);
  }, [videoRef]);

  // Sync cue layout & display mode when style settings change
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const customMode = subtitleStyleSettings.renderMode === "custom";
    for (let index = 0; index < video.textTracks.length; index++) {
      const track = video.textTracks[index];
      if (track.mode === "disabled") continue;
      applyCueLayout(track, subtitleStyleSettings);
      track.mode = customMode ? "hidden" : "showing";
    }
    subtitleStreamLoaderRef.current?.applyLayout(subtitleStyleSettings);
  }, [subtitleStyleSettings, videoRef]);

  // Reset subtitle when item changes
  useEffect(() => {
    const nextActiveSubtitleId = pickPreferredSubtitleId(
      itemSubtitles,
      itemFileId,
    );

    setRuntimeSubtitleTracks((prev) => (prev.length === 0 ? prev : []));
    setRemovedSubtitleIds((prev) => (prev.length === 0 ? prev : []));
    setAssFailedSubtitleId((prev) => (prev === null ? prev : null));
    setActiveSubtitleId((prev) =>
      prev === nextActiveSubtitleId ? prev : nextActiveSubtitleId,
    );
  }, [itemFileId, itemSubtitles]);

  // ── Callbacks ──

  const setSubtitle = useCallback(
    (id: string | null) => {
      setActiveSubtitleId(id);
      if (itemFileId) {
        saveSubtitleSelectionPreference(itemFileId, id);
      }
    },
    [itemFileId],
  );

  const updateSubtitleStyleSettings = useCallback(
    (nextSettings: Partial<SubtitleStyleSettings>) => {
      setSubtitleStyleSettings((prev) => ({
        ...prev,
        ...nextSettings,
      }));
    },
    [],
  );

  const registerSubtitleTrack = useCallback((track: SubtitleTrackItem) => {
    setRemovedSubtitleIds((prev) =>
      prev.filter((itemId) => itemId !== track.id),
    );
    setRuntimeSubtitleTracks((prev) => {
      const next = prev.filter((itemTrack) => itemTrack.id !== track.id);
      return [...next, track];
    });
  }, []);

  const removeSubtitleTrack = useCallback(
    (id: string) => {
      setRuntimeSubtitleTracks((prev) =>
        prev.filter((itemTrack) => itemTrack.id !== id),
      );
      setRemovedSubtitleIds((prev) => {
        if (prev.includes(id)) return prev;
        return [...prev, id];
      });
      setActiveSubtitleId((prev) => {
        const nextSubtitleId = prev === id ? null : prev;
        if (prev === id && itemFileId) {
          saveSubtitleSelectionPreference(itemFileId, nextSubtitleId);
        }
        return nextSubtitleId;
      });
    },
    [itemFileId],
  );

  return {
    subtitleTracks,
    activeSubtitleId,
    subtitleStyleSettings,
    setSubtitle,
    updateSubtitleStyleSettings,
    registerSubtitleTrack,
    removeSubtitleTrack,
    setNativeTextTracks,
  };
}
