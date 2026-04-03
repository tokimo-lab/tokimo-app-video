import { ALL_FORMATS, AudioBufferSink, Input, UrlSource } from "mediabunny";

/**
 * Plays decoded AC3/EAC3 audio through Web Audio API, synchronized with a
 * `<video>` element that handles video via DirectPlay.
 *
 * Architecture:
 *   video.src = streamUrl  →  Browser DirectPlay (video only)
 *   mediabunny Input(URL)  →  AudioBufferSink  →  AudioBuffer  →  AudioContext
 *
 * The browser handles ALL video I/O (buffering, seeking, range requests).
 * mediabunny only fetches and decodes the audio track (~640 kbps for AC3),
 * not the full file.  Seek is handled by restarting the audio iterator from
 * the new position using MKV cue-point-based seeking (no full re-download).
 *
 * The server provides an audio-only endpoint (`/audio-stream?track=N`) that
 * extracts a single audio track into a lightweight MKA file via codec-copy,
 * eliminating the ~95% bandwidth waste from reading the full interleaved MKV.
 *
 * Native audio silencing: uses `createMediaElementSource` to capture the
 * video element's native audio output and route it through a GainNode(0).
 * This is the only reliable way — `audioTracks[i].enabled = false` is
 * ignored by Chrome's Windows Media Foundation decoders.
 * Requires `crossOrigin="use-credentials"` on the `<video>` element.
 *
 * NOTE: An alternative approach using mediabunny's CanvasSink (full demux +
 * video decode + canvas rendering) would eliminate the double-fetch entirely,
 * but requires WebCodecs which mandates HTTPS — not available in dev mode.
 * It also has CPU/HDR tradeoffs for high-bitrate Remux content.
 */

/**
 * Shared per-element state for `createMediaElementSource` (can only be called
 * ONCE per element — subsequent calls throw). On construct we mute (gain=0),
 * on dispose we restore (gain=1). The pipeline stays connected at all times
 * so the video element never stalls.
 */
const nativeAudioCapture = new WeakMap<
  HTMLVideoElement,
  { ctx: AudioContext; muteGain: GainNode }
>();

/** Mute native audio output from a <video> element via Web Audio capture. */
export function muteNativeAudio(video: HTMLVideoElement) {
  let entry = nativeAudioCapture.get(video);
  if (!entry) {
    const ctx = new AudioContext();
    const source = ctx.createMediaElementSource(video);
    const muteGain = ctx.createGain();
    source.connect(muteGain);
    muteGain.connect(ctx.destination);
    entry = { ctx, muteGain };
    nativeAudioCapture.set(video, entry);
  }
  entry.muteGain.gain.value = 0;
  if (entry.ctx.state === "suspended") {
    entry.ctx.resume().catch(() => {});
  }
}

/** Restore native audio output from a <video> element. */
export function unmuteNativeAudio(video: HTMLVideoElement) {
  const entry = nativeAudioCapture.get(video);
  if (entry) {
    entry.muteGain.gain.value = 1;
    if (entry.ctx.state === "suspended") {
      entry.ctx.resume().catch(() => {});
    }
  }
}

export class Ac3AudioPlayer {
  private audioCtx: AudioContext;
  private gainNode: GainNode;
  private input: Input | null = null;
  private audioSink: AudioBufferSink | null = null;
  private disposed = false;
  /** Generation counter — stale pipelines check this and stop. */
  private gen = 0;
  /** True once init() completes and audioSink is ready. */
  private ready = false;
  /** If playFrom() is called before init() completes, store the pending time. */
  private pendingPlayFrom: number | null = null;

  // ── Audio scheduling state (mirrors official mediabunny example) ──
  /** audioCtx.currentTime when the current audio run was started. */
  private audioCtxStartTime = 0;
  /** Media timestamp (seconds) when the current audio run was started. */
  private mediaTimeAtStart = 0;
  /** All scheduled AudioBufferSourceNodes — stopped on seek/pause/dispose. */
  private queuedNodes = new Set<AudioBufferSourceNode>();
  /** The current async iterator — returned (cancelled) on seek/pause. */
  private bufferIterator: AsyncGenerator<
    { buffer: AudioBuffer; timestamp: number; duration: number },
    void,
    unknown
  > | null = null;

  constructor(private video: HTMLVideoElement) {
    // Silence the browser's native audio via Web Audio capture.
    muteNativeAudio(video);

    // Resume immediately while still in the user-gesture activation window;
    // deferring to playFrom()'s async callback is too late — the gesture expires.
    this.audioCtx = new AudioContext({ sampleRate: 48000 });
    if (this.audioCtx.state === "suspended") {
      this.audioCtx.resume().catch(() => {});
    }
    this.gainNode = this.audioCtx.createGain();
    this.gainNode.connect(this.audioCtx.destination);
    this.syncVolume();

    video.addEventListener("volumechange", this.onVolumeChange);
    video.addEventListener("play", this.onPlay);
    video.addEventListener("pause", this.onPause);
    video.addEventListener("seeking", this.onSeeking);
  }

  /**
   * Initialize the audio pipeline: probe the container and create the
   * AudioBufferSink.  Call this once per file — it runs in parallel with
   * the browser's video loading so there is zero delay when playFrom() is
   * called later.
   *
   * @param trackIndex  Which audio track to select (0-based). Defaults to 0
   *                    (primary audio track).
   */
  async init(streamUrl: string, trackIndex = 0): Promise<boolean> {
    this.input?.dispose();
    this.input = null;
    this.audioSink = null;
    this.ready = false;

    const input = new Input({
      formats: ALL_FORMATS,
      source: new UrlSource(streamUrl, { parallelism: 1 }),
    });
    this.input = input;

    try {
      const audioTracks = await input.getAudioTracks();
      if (!audioTracks.length || this.disposed) {
        input.dispose();
        return false;
      }

      const audioTrack =
        trackIndex >= 0 && trackIndex < audioTracks.length
          ? audioTracks[trackIndex]
          : audioTracks[0];

      const canDecode = await audioTrack.canDecode();
      if (!canDecode || this.disposed) {
        console.warn("[Mediabunny] Audio track cannot be decoded");
        input.dispose();
        return false;
      }

      // Match AudioContext sample rate to the track for correct pitch.
      if (this.audioCtx.sampleRate !== audioTrack.sampleRate) {
        this.audioCtx.close().catch(() => {});
        this.audioCtx = new AudioContext({
          sampleRate: audioTrack.sampleRate,
        });
        this.gainNode = this.audioCtx.createGain();
        this.gainNode.connect(this.audioCtx.destination);
        this.syncVolume();
      }

      this.audioSink = new AudioBufferSink(audioTrack);
      this.ready = true;

      console.log(
        `%c[Mediabunny]%c 🔊 Audio ready (codec: ${audioTrack.codec}, sr: ${audioTrack.sampleRate})`,
        "color:#f97316;font-weight:bold",
        "color:#22c55e",
      );

      // If playFrom() was called while we were probing, start now.
      if (this.pendingPlayFrom !== null) {
        const t = this.pendingPlayFrom;
        this.pendingPlayFrom = null;
        this.playFrom(t);
      }

      return true;
    } catch (err: unknown) {
      if (this.disposed) return false;
      const isDisposed =
        err instanceof Error && err.name === "InputDisposedError";
      if (!isDisposed) {
        console.error("[Mediabunny] Audio init error:", err);
      }
      return false;
    }
  }

  /**
   * Start (or restart) audio scheduling from a media timestamp.
   * This is instant — no network probing, just creates a new iterator
   * from the already-initialized AudioBufferSink.
   */
  playFrom(startTime: number) {
    if (this.disposed) return;

    // If init() hasn't finished yet, defer.
    if (!this.ready || !this.audioSink) {
      this.pendingPlayFrom = startTime;
      return;
    }

    this.stopAllAudio();
    this.gen++;
    const gen = this.gen;

    // Only resume AudioContext if video is actually playing.
    // (Seek while paused should NOT produce audio.)
    if (!this.video.paused && this.audioCtx.state === "suspended") {
      this.audioCtx.resume().catch(() => {});
    }

    // Preliminary sync point — will be refined when the first buffer arrives
    // (see runAudioIterator) to compensate for fetch+decode latency.
    this.audioCtxStartTime = this.audioCtx.currentTime;
    this.mediaTimeAtStart = startTime;

    // Create a fresh iterator and start the scheduling loop.
    this.bufferIterator = this.audioSink.buffers(startTime);
    void this.runAudioIterator(gen);
  }

  dispose() {
    this.disposed = true;
    this.stopAllAudio();
    this.gen++;
    this.input?.dispose();
    this.input = null;
    this.audioSink = null;
    this.video.removeEventListener("volumechange", this.onVolumeChange);
    this.video.removeEventListener("play", this.onPlay);
    this.video.removeEventListener("pause", this.onPause);
    this.video.removeEventListener("seeking", this.onSeeking);
    // Restore native audio output.
    unmuteNativeAudio(this.video);
    this.audioCtx.close().catch(() => {});
  }

  // ── internals ──

  /**
   * Iterate over decoded AudioBuffers and schedule them on AudioContext.
   * Re-syncs the clock on the first decoded buffer to compensate for the
   * fetch+decode latency between playFrom() and actual data arrival.
   */
  private async runAudioIterator(gen: number) {
    if (!this.bufferIterator) return;
    let isFirstBuffer = true;

    try {
      for await (const { buffer, timestamp } of this.bufferIterator) {
        if (this.disposed || gen !== this.gen) break;

        // Re-sync the clock when the first decoded buffer arrives.
        // Between playFrom() and now, the AudioContext clock advanced by the
        // fetch+decode latency (~200-400ms).  Without this re-sync, all
        // initial buffers would be "in the past" and get skipped, causing
        // audible delay.
        if (isFirstBuffer) {
          this.audioCtxStartTime = this.audioCtx.currentTime;
          this.mediaTimeAtStart = this.video.currentTime;
          isFirstBuffer = false;
        }

        const node = this.audioCtx.createBufferSource();
        node.buffer = buffer;
        node.connect(this.gainNode);

        // Schedule time = sync-point offset + (media timestamp - media start)
        const startTimestamp =
          this.audioCtxStartTime + timestamp - this.mediaTimeAtStart;

        if (startTimestamp >= this.audioCtx.currentTime) {
          // Audio is in the future — schedule it.
          node.start(startTimestamp);
        } else {
          // Audio is in the past — play only the remaining audible portion.
          const offset = this.audioCtx.currentTime - startTimestamp;
          if (offset < buffer.duration) {
            node.start(this.audioCtx.currentTime, offset);
          } else {
            // Entirely in the past — skip.
            continue;
          }
        }

        this.queuedNodes.add(node);
        node.onended = () => {
          this.queuedNodes.delete(node);
        };

        // Backpressure: if we're >1s ahead of playback, wait until caught up.
        const playbackTime = this.getPlaybackTime();
        if (timestamp - playbackTime >= 1) {
          await new Promise<void>((resolve) => {
            const id = setInterval(() => {
              if (
                this.disposed ||
                gen !== this.gen ||
                timestamp - this.getPlaybackTime() < 1
              ) {
                clearInterval(id);
                resolve();
              }
            }, 100);
          });
        }
      }
    } catch (err: unknown) {
      if (this.disposed || gen !== this.gen) return;
      const isDisposed =
        err instanceof Error && err.name === "InputDisposedError";
      if (!isDisposed) {
        console.error("[Mediabunny] Audio iterator error:", err);
      }
    }
  }

  /** Current playback time using AudioContext as the clock (matches official example). */
  private getPlaybackTime(): number {
    if (!this.video.paused) {
      return (
        this.audioCtx.currentTime -
        this.audioCtxStartTime +
        this.mediaTimeAtStart
      );
    }
    return this.video.currentTime;
  }

  /** Stop all scheduled audio nodes and cancel the iterator. */
  private stopAllAudio() {
    void this.bufferIterator?.return();
    this.bufferIterator = null;
    for (const node of this.queuedNodes) {
      node.stop();
    }
    this.queuedNodes.clear();
  }

  /**
   * Track volume changes from the player UI and apply to our GainNode.
   * The <video> element's native audio is silenced via
   * createMediaElementSource, so video.volume only affects our GainNode.
   */
  private onVolumeChange = () => {
    this.syncVolume();
  };

  private syncVolume() {
    const vol = this.video.muted ? 0 : this.video.volume;
    this.gainNode.gain.value = vol ** 2;
  }

  private onPlay = () => {
    if (!this.audioSink || this.disposed) return;

    if (this.audioCtx.state === "suspended") {
      this.audioCtx.resume().catch(() => {});
    }

    // Re-sync clock and restart iterator from current video position.
    this.stopAllAudio();
    this.gen++;
    const gen = this.gen;
    this.audioCtxStartTime = this.audioCtx.currentTime;
    this.mediaTimeAtStart = this.video.currentTime;
    this.bufferIterator = this.audioSink.buffers(this.video.currentTime);
    void this.runAudioIterator(gen);
  };

  private onPause = () => {
    // Stop all queued nodes immediately (official example pattern).
    this.stopAllAudio();
    if (this.audioCtx.state === "running") {
      this.audioCtx.suspend().catch(() => {});
    }
  };

  private onSeeking = () => {
    // Only restart audio if video is playing.
    // If paused, onPlay will start audio when the user resumes.
    if (!this.video.paused) {
      this.playFrom(this.video.currentTime);
    } else {
      // Just stop stale audio — onPlay will restart from the new position.
      this.stopAllAudio();
    }
  };
}
