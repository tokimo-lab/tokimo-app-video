// PGS canvas renderer — renders parsed PGS display sets onto an overlay canvas.
// Uses requestVideoFrameCallback (when available) to sync subtitles to the
// actual displayed video frame rather than video.currentTime, which can diverge
// from the real frame PTS after HLS seeks (keyframe snapping).

import {
  base64ToBytes,
  decodeRle,
  type ParsedDisplaySet,
  parseDisplaySet,
  ycbcrToRgba,
} from "./pgs-parser";

export class PgsCanvasRenderer {
  private readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  private readonly video: HTMLVideoElement;
  private readonly pendingDisplaySets: Map<number, ParsedDisplaySet> =
    new Map();
  private rafId: number | null = null;
  private lastRenderedTimeMs = -1;
  private disposed = false;
  // Timestamp of last feedDisplaySet — used to defer rendering while
  // SSE snapshot events are still arriving in a burst.
  private lastFeedTs = 0;
  private readonly onSeeking: () => void;
  // Actual displayed frame media time from requestVideoFrameCallback.
  // More accurate than video.currentTime after HLS seeks because
  // currentTime reports the seek target while the actual frame is the
  // nearest keyframe (which can be seconds earlier).
  private frameMediaTimeMs = -1;
  private frameCallbackId: number | null = null;
  private readonly hasFrameCallback: boolean;

  constructor(video: HTMLVideoElement, container: HTMLElement) {
    this.video = video;
    this.canvas = document.createElement("canvas");
    this.canvas.dataset.pgsOverlay = "true";
    this.canvas.style.cssText =
      "position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:10;";
    container.style.position = "relative";

    // Remove any orphaned PGS canvases from a previous renderer
    container
      .querySelectorAll('canvas[data-pgs-overlay="true"]')
      .forEach((c) => {
        c.remove();
      });
    container.appendChild(this.canvas);

    const ctx = this.canvas.getContext("2d");
    if (!ctx) throw new Error("[PgsCanvasRenderer] cannot get 2d context");
    this.ctx = ctx;

    // On seek: clear canvas visuals, invalidate frame time
    this.onSeeking = () => {
      this.lastRenderedTimeMs = -1;
      this.frameMediaTimeMs = -1;
      this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    };
    this.video.addEventListener("seeking", this.onSeeking);

    // Use requestVideoFrameCallback for accurate frame-level timing
    this.hasFrameCallback = "requestVideoFrameCallback" in video;
    if (this.hasFrameCallback) {
      this.startFrameCallback();
    }

    this.startRenderLoop();
  }

  private startFrameCallback(): void {
    const onFrame = (_now: number, metadata: { mediaTime?: number }): void => {
      if (this.disposed) return;
      if (metadata.mediaTime != null) {
        this.frameMediaTimeMs = metadata.mediaTime * 1000;
      }
      this.frameCallbackId = (
        this.video as HTMLVideoElement & {
          requestVideoFrameCallback: (
            cb: (now: number, md: { mediaTime?: number }) => void,
          ) => number;
        }
      ).requestVideoFrameCallback(onFrame);
    };
    this.frameCallbackId = (
      this.video as HTMLVideoElement & {
        requestVideoFrameCallback: (
          cb: (now: number, md: { mediaTime?: number }) => void,
        ) => number;
      }
    ).requestVideoFrameCallback(onFrame);
  }

  feedDisplaySet(timeMs: number, base64Data: string): void {
    if (this.disposed) return;
    this.lastFeedTs = performance.now();

    try {
      const bytes = base64ToBytes(base64Data);
      const ds = parseDisplaySet(bytes, timeMs);
      this.pendingDisplaySets.set(timeMs, ds);
    } catch (err) {
      console.debug("[PGS] malformed display set", err);
    }
  }

  private startRenderLoop(): void {
    const loop = () => {
      this.rafId = requestAnimationFrame(loop);
      this.renderFrame();
    };
    this.rafId = requestAnimationFrame(loop);
  }

  private renderFrame(): void {
    if (this.video.seeking || this.video.readyState < 2) {
      this.lastRenderedTimeMs = -1;
      return;
    }

    // Defer while SSE events are arriving in a burst (initial snapshot).
    if (this.lastFeedTs > 0 && performance.now() - this.lastFeedTs < 150) {
      this.lastRenderedTimeMs = -1;
      return;
    }

    // Prefer actual frame mediaTime over currentTime for subtitle sync.
    // After HLS seek, currentTime = seek target but actual frame = nearest
    // keyframe (often 2-5s earlier). frameMediaTimeMs reflects the real frame.
    const frameMs = this.frameMediaTimeMs;
    const ctMs = this.video.currentTime * 1000;
    const currentMs = frameMs >= 0 ? frameMs : ctMs;

    // Find the most recent display set at or before currentMs
    let activeDs: ParsedDisplaySet | null = null;
    let activeTimeMs = -1;
    for (const [tms, ds] of this.pendingDisplaySets) {
      if (tms <= currentMs && tms > activeTimeMs) {
        activeTimeMs = tms;
        activeDs = ds;
      }
    }

    // Content DS staleness: cap display at 6s.
    // PGS dialogue subtitles typically last 2-5s before a clear DS replaces
    // them. For TS files, the stream tap delivers DS progressively — after
    // seek, a content DS may arrive before its clear DS, causing it to
    // linger incorrectly. 6s filters these stale subtitles while allowing
    // normal-length displays. During normal playback the clear DS arrives
    // well before 6s so this doesn't affect timing.
    if (
      activeDs?.pcs &&
      activeDs.pcs.objects.length > 0 &&
      currentMs - activeTimeMs > 6_000
    ) {
      activeDs = null;
    }

    if (activeDs?.endMs != null && currentMs > activeDs.endMs) {
      activeDs = null;
    }

    const renderKey = activeDs ? activeDs.timeMs : -1;
    if (renderKey === this.lastRenderedTimeMs) return;

    this.lastRenderedTimeMs = renderKey;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

    if (!activeDs) return;

    const pcs = activeDs.pcs;
    if (!pcs) return;

    const pcsW = pcs.width || 1920;
    const pcsH = pcs.height || 1080;
    if (this.canvas.width !== pcsW || this.canvas.height !== pcsH) {
      this.canvas.width = pcsW;
      this.canvas.height = pcsH;
    }

    if (pcs.objects.length === 0) return;

    for (const obj of pcs.objects) {
      const ods = activeDs.objects.get(obj.objectId);
      if (!ods || ods.width === 0 || ods.height === 0) continue;

      try {
        const pixels = decodeRle(ods.rleData, ods.width, ods.height);
        const imgData = ycbcrToRgba(
          activeDs.palette,
          pixels,
          ods.width,
          ods.height,
        );
        this.ctx.putImageData(imgData, obj.x, obj.y);
      } catch {
        // ignore render errors for individual objects
      }
    }
  }

  /** Clear all cached display sets and the canvas. Called on seek. */
  clear(): void {
    this.pendingDisplaySets.clear();
    this.lastRenderedTimeMs = -1;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
  }

  dispose(): void {
    this.disposed = true;
    this.video.removeEventListener("seeking", this.onSeeking);
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    if (this.frameCallbackId !== null && this.hasFrameCallback) {
      (
        this.video as HTMLVideoElement & {
          cancelVideoFrameCallback: (id: number) => void;
        }
      ).cancelVideoFrameCallback(this.frameCallbackId);
      this.frameCallbackId = null;
    }
    this.canvas.remove();
    this.pendingDisplaySets.clear();
  }
}
