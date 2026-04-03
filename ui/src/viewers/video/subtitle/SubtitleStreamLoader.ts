import {
  getSubtitleCueLine,
  type SubtitleStyleSettings,
} from "@/lib/player-subtitles";
import { type PgsCanvasRenderer } from "./PgsCanvasRenderer";

// ── Helpers ───────────────────────────────────────────────────────────────────

function isVttCue(cue: TextTrackCue): cue is VTTCue {
  return typeof (cue as VTTCue).line !== "undefined";
}

function applyCueLayout(
  track: TextTrack,
  settings: SubtitleStyleSettings,
): void {
  if (!track.cues) return;
  for (let i = 0; i < track.cues.length; i++) {
    const cue = track.cues[i];
    if (!cue || !isVttCue(cue)) continue;
    cue.snapToLines = false;
    cue.line = getSubtitleCueLine(settings.position);
    cue.position = 50;
    cue.size = 90;
    cue.align = "center";
  }
}

// ── SubtitleStreamLoader ──────────────────────────────────────────────────────
//
// Connects to the Rust SSE endpoint. Rust pushes subtitle events as they are
// extracted from the player's byte stream. All text cleaning (ASS stripping,
// HTML tag removal) is done server-side — this loader just adds VTTCues.
//
// The SSE connection is lazy — call `ensureConnected()` when the video
// actually starts playing to avoid wasting resources before playback.

/** Rust server base URL for direct SSE connection */
import { rustUrl } from "@/lib/rust-api-runtime";

/** Keep at most this many cues in the TextTrack to avoid memory bloat. */
const MAX_CUES = 500;

export class SubtitleStreamLoader {
  private track: TextTrack | null;
  private destroyed = false;
  private connected = false;
  private eventSource: EventSource | null = null;
  private readonly knownTimestamps = new Set<number>();
  private pgsRenderer: PgsCanvasRenderer | null = null;
  private readonly isPgs: boolean;
  // Buffer PGS events that arrive before the renderer is attached
  private pgsBuffer: Array<{ timeMs: number; data: string }> = [];
  /** Cached settings so newly created cues inherit the correct layout. */
  private currentSettings: SubtitleStyleSettings | null = null;

  readonly subtitleId: string;

  constructor(
    _video: HTMLVideoElement,
    subtitleId: string,
    language: string,
    label: string,
    private readonly accessToken: string,
    format?: string,
  ) {
    this.subtitleId = subtitleId;
    this.isPgs = format === "pgs" || format === "pgssub" || format === "sup";
    if (!this.isPgs) {
      this.track = _video.addTextTrack(
        "subtitles",
        label || language,
        language,
      );
      this.track.mode = "showing";
    } else {
      this.track = null;
    }
    // SSE connection is deferred — call ensureConnected() when playback starts.
  }

  get isDestroyed(): boolean {
    return this.destroyed;
  }

  getTrack(): TextTrack | null {
    return this.track;
  }

  /** No-op — kept for interface compat with VideoPlayer.tsx */
  async preload(_currentTimeMs: number): Promise<void> {}

  /**
   * Called after the video's `seeked` event fires.
   * Clears stale VTTCues and refetches from the GET endpoint so the
   * displayed subtitles match the new playback position.
   *
   * Events already in the server cache but never broadcast (because they
   * were delivered in the original SSE snapshot or deduped) would otherwise
   * be permanently lost after a seek — this GET-refetch closes that gap.
   */
  onSeek(currentTimeSec: number): void {
    if (this.destroyed || this.isPgs) return;
    this.clearCues();
    void this.refetchCues(currentTimeSec);
  }

  /** Remove all VTTCues from the TextTrack and reset dedup state. */
  private clearCues(): void {
    const track = this.track;
    if (!track?.cues) return;
    while (track.cues.length > 0) {
      track.removeCue(track.cues[0]);
    }
    this.knownTimestamps.clear();
  }

  /**
   * Fetch subtitle events around `anchorSec` from the REST endpoint
   * and add them as VTTCues.  This covers events the SSE snapshot
   * already delivered (which won't be broadcast again).
   */
  private async refetchCues(anchorSec: number): Promise<void> {
    const startMs = Math.max(0, (anchorSec - 30) * 1000);
    const endMs = (anchorSec + 300) * 1000;
    const params = new URLSearchParams({
      startMs: String(startMs),
      endMs: String(endMs),
    });
    if (this.accessToken) {
      params.set("accessToken", this.accessToken);
    }
    const url = rustUrl(
      `/api/apps/subtitles/${encodeURIComponent(this.subtitleId)}/events?${params.toString()}`,
    );
    try {
      const res = await fetch(url);
      if (!res.ok) return;
      const body = (await res.json()) as {
        events: Array<{
          timeMs: number;
          endMs: number | null;
          text: string | null;
        }>;
      };
      if (this.destroyed) return;
      for (const ev of body.events) {
        if (!ev.text) continue;
        if (this.knownTimestamps.has(ev.timeMs)) continue;
        this.knownTimestamps.add(ev.timeMs);
        const startSec = ev.timeMs / 1000;
        const endSec = ev.endMs != null ? ev.endMs / 1000 : startSec + 3;
        this.addCuePruned(startSec, endSec, ev.text);
      }
    } catch {
      // Network error — SSE will continue delivering new events.
    }
  }

  /** Open the SSE connection if not already connected. Idempotent. */
  ensureConnected(): void {
    if (this.connected || this.destroyed) return;
    this.connected = true;
    this.connect();
  }

  private connect(): void {
    if (this.destroyed) return;

    const params = new URLSearchParams();
    if (this.accessToken) {
      params.set("accessToken", this.accessToken);
    }
    const url = rustUrl(
      `/api/apps/subtitles/${encodeURIComponent(this.subtitleId)}/sse?${params.toString()}`,
    );

    const es = new EventSource(url);
    this.eventSource = es;

    es.onmessage = (msg) => {
      if (this.destroyed) return;

      try {
        const ev = JSON.parse(msg.data) as {
          timeMs: number;
          endMs: number | null;
          text: string | null;
          data: string | null;
        };

        if (!ev.text) return;
        if (this.knownTimestamps.has(ev.timeMs)) return;

        const startSec = ev.timeMs / 1000;
        const endSec = ev.endMs != null ? ev.endMs / 1000 : startSec + 3;

        this.knownTimestamps.add(ev.timeMs);
        this.addCuePruned(startSec, endSec, ev.text);
      } catch {
        return;
      }
    };

    es.onerror = (e) => {
      console.warn("[SubtitleSSE] error", es.readyState, e);
    };

    es.addEventListener("pgs", (msg: MessageEvent<string>) => {
      if (this.destroyed) return;
      try {
        const ev = JSON.parse(msg.data) as {
          timeMs: number;
          endMs: number | null;
          data: string | null;
        };
        if (!ev.data) return;
        if (this.pgsRenderer) {
          this.pgsRenderer.feedDisplaySet(ev.timeMs, ev.data);
        } else {
          this.pgsBuffer.push({ timeMs: ev.timeMs, data: ev.data });
        }
      } catch {
        return;
      }
    });
  }

  /** Add a VTTCue, evicting the oldest cues when the track exceeds MAX_CUES. */
  private addCuePruned(startSec: number, endSec: number, text: string): void {
    const track = this.track;
    if (!track) return;

    const cue = new VTTCue(startSec, endSec, text);
    if (this.currentSettings) {
      cue.snapToLines = false;
      cue.line = getSubtitleCueLine(this.currentSettings.position);
      cue.position = 50;
      cue.size = 90;
      cue.align = "center";
    }
    track.addCue(cue);

    // Prune oldest cues (by startTime) when we exceed the budget.
    if (track.cues && track.cues.length > MAX_CUES) {
      const toRemove = track.cues.length - MAX_CUES;
      // Collect cues sorted by startTime, remove the earliest ones.
      const sorted: VTTCue[] = [];
      for (let i = 0; i < track.cues.length; i++) {
        sorted.push(track.cues[i] as VTTCue);
      }
      sorted.sort((a, b) => a.startTime - b.startTime);
      for (let i = 0; i < toRemove; i++) {
        track.removeCue(sorted[i]);
        this.knownTimestamps.delete(Math.round(sorted[i].startTime * 1000));
      }
    }
  }

  setPgsRenderer(renderer: PgsCanvasRenderer | null): void {
    this.pgsRenderer = renderer;
    // Replay any buffered PGS events
    if (renderer && this.pgsBuffer.length > 0) {
      for (const ev of this.pgsBuffer) {
        renderer.feedDisplaySet(ev.timeMs, ev.data);
      }
      this.pgsBuffer = [];
    }
  }

  applyLayout(settings: SubtitleStyleSettings): void {
    this.currentSettings = settings;
    if (this.track) applyCueLayout(this.track, settings);
  }

  destroy(): void {
    this.destroyed = true;
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.pgsRenderer = null;
    this.pgsBuffer = [];
    if (this.track) this.track.mode = "disabled";
  }
}
