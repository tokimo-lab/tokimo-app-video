/**
 * SubtitleCustomOverlay — 自定义字幕覆盖层渲染器
 *
 * 当 renderMode === "custom" 时，监听 TextTrack(mode="hidden") 的 cuechange 事件，
 * 以绝对定位 div 渲染字幕，可完整应用所有 CSS 样式（包括背景色）。
 */
import { type RefObject, useEffect, useState } from "react";
import {
  getTextShadowCss,
  type SubtitleStyleSettings,
} from "@/lib/player-subtitles";

interface SubtitleCustomOverlayProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  settings: SubtitleStyleSettings;
}

function getPositionStyle(
  position: SubtitleStyleSettings["position"],
): React.CSSProperties {
  switch (position) {
    case "top":
      return { top: "8%", bottom: "auto" };
    case "middle":
      return { top: "50%", bottom: "auto", transform: "translateY(-50%)" };
    default:
      return { bottom: "8%", top: "auto" };
  }
}

export function SubtitleCustomOverlay({
  videoRef,
  settings,
}: SubtitleCustomOverlayProps) {
  const [activeCues, setActiveCues] = useState<string[]>([]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    const vid = video; // narrowed non-null ref for closures

    const handlers = new Map<TextTrack, () => void>();

    function handleCueChange(track: TextTrack) {
      if (!track.activeCues) {
        setActiveCues([]);
        return;
      }
      const cues: string[] = [];
      for (let i = 0; i < track.activeCues.length; i++) {
        const cue = track.activeCues[i];
        if (cue instanceof VTTCue && cue.text) {
          cues.push(cue.text);
        }
      }
      setActiveCues(cues);
    }

    function attachTrack(track: TextTrack) {
      if (track.mode === "disabled") return;
      const handler = () => handleCueChange(track);
      handlers.set(track, handler);
      track.addEventListener("cuechange", handler);
    }

    function detachAll() {
      for (const [track, handler] of handlers) {
        track.removeEventListener("cuechange", handler);
      }
      handlers.clear();
    }

    function syncTracks() {
      detachAll();
      const tracks = vid.textTracks;
      for (let i = 0; i < tracks.length; i++) {
        attachTrack(tracks[i]);
      }
    }

    syncTracks();
    vid.textTracks.addEventListener("change", syncTracks);
    vid.textTracks.addEventListener("addtrack", syncTracks);
    vid.textTracks.addEventListener("removetrack", syncTracks);

    return () => {
      vid.textTracks.removeEventListener("change", syncTracks);
      vid.textTracks.removeEventListener("addtrack", syncTracks);
      vid.textTracks.removeEventListener("removetrack", syncTracks);
      detachAll();
    };
  }, [videoRef]);

  if (activeCues.length === 0) return null;

  const posStyle = getPositionStyle(settings.position);

  return (
    <div
      className="pointer-events-none absolute inset-x-0 z-30 flex flex-col items-center gap-1 px-[5%]"
      style={posStyle}
    >
      {activeCues.map((cueText, idx) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: subtitle cues are ordered by time; index is the only stable key
          key={`cue-${idx}`}
          className="inline-block max-w-full whitespace-pre-wrap break-words text-center leading-tight"
          style={{
            color: settings.color,
            fontSize: `${settings.fontSize}px`,
            fontFamily: settings.fontFamily,
            fontWeight: settings.fontWeight,
            backgroundColor: settings.backgroundColor,
            textShadow: getTextShadowCss(settings.textShadow),
            padding:
              settings.backgroundColor !== "rgba(0,0,0,0)" &&
              settings.backgroundColor !== "transparent"
                ? "0.1em 0.4em"
                : undefined,
            borderRadius:
              settings.backgroundColor !== "rgba(0,0,0,0)" &&
              settings.backgroundColor !== "transparent"
                ? "0.15em"
                : undefined,
          }}
          // biome-ignore lint/security/noDangerouslySetInnerHtml: cue text rendered from trusted subtitle data
          dangerouslySetInnerHTML={{
            __html: cueText
              .replace(/&/g, "&amp;")
              .replace(/</g, "&lt;")
              .replace(/>/g, "&gt;")
              .replace(/\\N/gi, "<br>")
              .replace(/\n/g, "<br>"),
          }}
        />
      ))}
    </div>
  );
}
