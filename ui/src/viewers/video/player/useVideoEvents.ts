/**
 * useVideoEvents — binds HTML5 video element events to React state.
 *
 * Extracts the event-listener wiring from VideoPlayer into a focused hook.
 * All state setters from useState are stable (React guarantee) and excluded
 * from the dependency array. Only `setIsPlaying`, `setPosition` (from
 * PlayerContext) and `itemDuration` are meaningful deps.
 */
import type {
  Dispatch,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";
import { useEffect } from "react";
import { isPendingSeekSettled } from "@/lib/player-seek";

function areBufferedRangesEqual(
  prev: { start: number; end: number }[],
  next: { start: number; end: number }[],
): boolean {
  return (
    prev.length === next.length &&
    prev.every(
      (range, index) =>
        range.start === next[index]?.start && range.end === next[index]?.end,
    )
  );
}

export interface UseVideoEventsParams {
  videoRef: RefObject<HTMLVideoElement | null>;
  pendingSeekTimeRef: MutableRefObject<number | null>;
  itemDuration: number | null;
  setCurrentTime: Dispatch<SetStateAction<number>>;
  setDuration: Dispatch<SetStateAction<number>>;
  setPaused: Dispatch<SetStateAction<boolean>>;
  setMuted: Dispatch<SetStateAction<boolean>>;
  setVolume: Dispatch<SetStateAction<number>>;
  setWaiting: Dispatch<SetStateAction<boolean>>;
  setStarted: Dispatch<SetStateAction<boolean>>;
  setBufferedRanges: Dispatch<SetStateAction<{ start: number; end: number }[]>>;
  /** PlayerContext: sync playing state back to global context. */
  setIsPlaying: (v: boolean) => void;
  /** PlayerContext: report current playback position. */
  setPosition: (pos: number) => void;
}

// NOTE: Video audio spectrum analysis is intentionally NOT done here.
// createMediaElementSource() takes exclusive ownership of the <video>
// element's audio output, routing it through Web Audio API.  If the
// AudioContext is suspended (Chrome autoplay policy) or interacts badly
// with MSE (hls.js), the entire video pipeline stalls — currentTime
// freezes even with plenty of buffered data.  The menubar visualizer
// only activates for the music player (which uses its own AnalyserNode
// from WasmAudioEngine, independent of any <audio>/<video> element).

export function useVideoEvents({
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
}: UseVideoEventsParams): void {
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const onTimeUpdate = () => {
      const nextTime = Math.max(0, video.currentTime);
      const pendingSeekTime = pendingSeekTimeRef.current;

      if (
        pendingSeekTime !== null &&
        !isPendingSeekSettled(nextTime, pendingSeekTime)
      ) {
        return;
      }

      if (pendingSeekTime !== null) {
        pendingSeekTimeRef.current = null;
      }

      setCurrentTime(nextTime);
      setPosition(nextTime);
    };
    const onDurationChange = () => {
      const raw = video.duration;
      // fragmented-MP4 / live HLS may report 0 or Infinity initially
      if (raw && Number.isFinite(raw)) setDuration(raw);
      else if (itemDuration) setDuration(itemDuration);
    };
    const onPlay = () => {
      setPaused(false);
      setIsPlaying(true);
    };
    const onPause = () => {
      setPaused(true);
      setIsPlaying(false);
    };
    const onWaiting = () => setWaiting(true);
    const onPlaying = () => {
      setWaiting(false);
      setStarted(true);
    };
    const onCanPlay = () => {
      setWaiting(false);
    };
    const onVolumeChange = () => {
      setVolume(video.volume);
      setMuted(video.muted);
    };
    const onProgress = () => {
      const ranges: { start: number; end: number }[] = [];
      for (let i = 0; i < video.buffered.length; i++) {
        const start = video.buffered.start(i);
        const end = video.buffered.end(i);
        if (Number.isFinite(start) && Number.isFinite(end)) {
          ranges.push({ start, end });
        }
      }
      setBufferedRanges((prev) =>
        areBufferedRangesEqual(prev, ranges) ? prev : ranges,
      );
    };
    const onEnded = () => {
      setIsPlaying(false);
    };

    video.addEventListener("timeupdate", onTimeUpdate);
    video.addEventListener("durationchange", onDurationChange);
    video.addEventListener("play", onPlay);
    video.addEventListener("pause", onPause);
    video.addEventListener("waiting", onWaiting);
    video.addEventListener("playing", onPlaying);
    video.addEventListener("canplay", onCanPlay);
    video.addEventListener("volumechange", onVolumeChange);
    video.addEventListener("progress", onProgress);
    video.addEventListener("ended", onEnded);

    return () => {
      video.removeEventListener("timeupdate", onTimeUpdate);
      video.removeEventListener("durationchange", onDurationChange);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("waiting", onWaiting);
      video.removeEventListener("playing", onPlaying);
      video.removeEventListener("canplay", onCanPlay);
      video.removeEventListener("volumechange", onVolumeChange);
      video.removeEventListener("progress", onProgress);
      video.removeEventListener("ended", onEnded);
    };
  }, [
    setIsPlaying,
    setPosition,
    itemDuration,
    pendingSeekTimeRef,
    setBufferedRanges,
    setCurrentTime,
    setDuration,
    setMuted,
    setPaused,
    setStarted,
    setVolume,
    setWaiting,
    videoRef,
  ]);
}
