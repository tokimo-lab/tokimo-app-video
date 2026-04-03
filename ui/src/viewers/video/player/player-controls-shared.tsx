/**
 * player-controls-shared — shared hooks, tooltip, and format helpers
 * for CustomVideoControls and its extracted sub-components.
 */
import { Tooltip } from "@tokiomo/components";
import type { ReactElement, ReactNode, RefObject } from "react";
import { useEffect, useRef, useState } from "react";
import {
  FlagBadge,
  formatChannels,
  formatLanguage,
  formatSubtitleFormat,
  formatSubtitleSourceType,
  getLanguageCountryCode,
  LanguageLabel,
} from "@/lib/media-language";
import type { AudioTrackItem, SubtitleTrackItem } from "@/system";
import { classifyAudioTrack } from "@/system/media/codec-detection";

// ── Format helpers ───────────────────────────────────────────────────────────

const AUDIO_MODE_TAG: Record<
  ReturnType<typeof classifyAudioTrack>,
  { label: string; className: string }
> = {
  native: {
    label: "浏览器原生",
    className: "bg-green-500/20 text-green-300 ring-green-400/30",
  },
  mediabunny: {
    label: "浏览器解码",
    className: "bg-blue-500/20 text-blue-300 ring-blue-400/30",
  },
  "server-transcode": {
    label: "服务端解码",
    className: "bg-orange-500/20 text-orange-300 ring-orange-400/30",
  },
};

export function fmtTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function renderAudioTrackFacts(
  track: AudioTrackItem,
  locale: string,
): ReactNode {
  const facts = [
    { label: "编码", value: track.codec?.toUpperCase() ?? null },
    { label: "声道", value: formatChannels(track.channels, locale) },
    {
      label: "码率",
      value: typeof track.bitrate === "number" ? `${track.bitrate} kbps` : null,
    },
  ].filter((fact): fact is { label: string; value: string } =>
    Boolean(fact.value),
  );

  const mode = classifyAudioTrack(track.codec ?? null);
  const tag = AUDIO_MODE_TAG[mode];

  return (
    <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[10px] leading-none text-white/45">
      <span
        className={`inline-flex shrink-0 items-center rounded px-1 py-0.5 text-[9px] font-medium leading-none ring-1 ${tag.className}`}
      >
        {tag.label}
      </span>
      {facts.map((fact) => (
        <span key={fact.label} className="inline-flex items-center gap-1">
          <span>{fact.label}</span>
          <span className="text-white/25">·</span>
          <span className="text-white/65">{fact.value}</span>
        </span>
      ))}
    </span>
  );
}

export function formatCompactChannels(
  channels: number | null | undefined,
  locale: string,
): string | null {
  return formatChannels(channels, locale);
}

export function formatCompactSubtitleFormat(format: string): string {
  const normalized = format.trim().toLowerCase();
  switch (normalized) {
    case "hdmv_pgs_subtitle":
    case "pgs":
    case "sup":
      return "PGS";
    case "subrip":
      return "SRT";
    default:
      return format.toUpperCase();
  }
}

export function renderAudioTriggerSummary(
  track: AudioTrackItem | undefined,
  locale: string,
): ReactNode {
  if (!track) return "音轨";

  const summary = [
    track.codec?.toUpperCase() ?? null,
    formatCompactChannels(track.channels, locale),
  ]
    .filter((value): value is string => Boolean(value))
    .join(" ");

  return (
    <span className="inline-flex min-w-0 items-center gap-1.5">
      <FlagBadge
        countryCode={getLanguageCountryCode(track.language)}
        className="h-3 w-[16px] shrink-0 overflow-hidden rounded-[2px] border border-black/10"
      />
      <span className="truncate">{summary || "音轨"}</span>
    </span>
  );
}

export function renderSubtitleTriggerSummary(
  track: SubtitleTrackItem | undefined,
): ReactNode {
  if (!track) return null;

  return (
    <span className="inline-flex min-w-0 items-center gap-1.5">
      <FlagBadge
        countryCode={getLanguageCountryCode(track.language)}
        className="h-3 w-[16px] shrink-0 overflow-hidden rounded-[2px] border border-black/10"
      />
      <span className="truncate">
        {formatCompactSubtitleFormat(track.format)}
      </span>
    </span>
  );
}

export function renderAudioTrackLabel(
  track: AudioTrackItem,
  locale: string,
): ReactNode {
  const facts = renderAudioTrackFacts(track, locale);
  const languageValue = formatLanguage(track.language, locale);

  return (
    <span className="flex min-w-0 flex-col">
      <span className="flex min-w-0 items-center gap-1 text-[10px] leading-none text-white/45">
        <span>语言</span>
        <span className="text-white/25">·</span>
        {languageValue ? (
          <LanguageLabel
            language={track.language}
            locale={locale}
            fallback={`音轨 ${track.id + 1}`}
            className="inline-flex min-w-0 items-center gap-1.5 leading-none"
            textClassName="truncate leading-none"
            flagClassName="h-3.5 w-[18px] shrink-0 overflow-hidden rounded-[2px] border border-black/10"
          />
        ) : (
          <span className="truncate leading-none text-white/65">{`音轨 ${track.id + 1}`}</span>
        )}
      </span>
      {facts}
    </span>
  );
}

export function renderSubtitleTrackLabel(
  track: SubtitleTrackItem,
  locale: string,
): ReactNode {
  const details = [
    formatSubtitleSourceType(track.sourceType, locale),
    formatSubtitleFormat(track.format, locale),
    track.label.trim() && track.label.trim() !== track.language.trim()
      ? track.label.trim()
      : null,
  ].filter((value): value is string => Boolean(value));

  return (
    <span className="flex min-w-0 flex-col">
      <LanguageLabel
        language={track.language}
        locale={locale}
        fallback={track.label || "字幕"}
        className="inline-flex min-w-0 items-center gap-1.5 leading-none"
        textClassName="truncate leading-none"
      />
      <span className="truncate text-[10px] text-white/45">
        {details.join(" · ")}
      </span>
    </span>
  );
}

export function subtitleGroupLabel(sourceType?: string): string {
  if (sourceType === "embedded") return "内置字幕";
  if (sourceType === "downloaded") return "已下载字幕";
  if (sourceType === "external") return "外挂字幕";
  return "其他字幕";
}

export function groupSubtitleTracks<
  T extends {
    id: string;
    sourceType?: string;
  },
>(tracks: T[]): Array<{ key: string; title: string; items: T[] }> {
  const groupOrder = ["embedded", "external", "downloaded", "other"] as const;
  const grouped = new Map<string, T[]>();

  for (const track of tracks) {
    const key =
      track.sourceType === "embedded" ||
      track.sourceType === "external" ||
      track.sourceType === "downloaded"
        ? track.sourceType
        : "other";
    const items = grouped.get(key) ?? [];
    items.push(track);
    grouped.set(key, items);
  }

  const result: Array<{ key: string; title: string; items: T[] }> = [];

  for (const key of groupOrder) {
    const items = grouped.get(key);
    if (!items?.length) {
      continue;
    }
    result.push({
      key,
      title: subtitleGroupLabel(key),
      items,
    });
  }

  return result;
}

// ── Shared control components ────────────────────────────────────────────────

export function PlayerControlTooltip({
  title,
  children,
}: {
  title: string;
  children: ReactElement;
}) {
  return (
    <Tooltip
      title={title}
      mouseEnterDelay={0}
      mouseLeaveDelay={0}
      color="bg-black/65 text-white backdrop-blur-2xl ring-1 ring-white/10"
    >
      {children}
    </Tooltip>
  );
}

// ── Shared hooks ─────────────────────────────────────────────────────────────

export function useDismissOnOutsidePointerDown(
  open: boolean,
  onDismiss: () => void,
  ignoredSelectors: string[] = [],
  extraRefs: RefObject<Element | null>[] = [],
) {
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (containerRef.current?.contains(target)) {
        return;
      }
      if (extraRefs.some((r) => r.current?.contains(target))) {
        return;
      }
      if (
        target instanceof Element &&
        ignoredSelectors.some((selector) => target.closest(selector))
      ) {
        return;
      }

      onDismiss();
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
    };
  }, [ignoredSelectors, onDismiss, open, extraRefs]);

  return containerRef;
}

/**
 * Tracks the fixed-positioned coordinates for a portal dropdown,
 * updating continuously via rAF while open (handles draggable floating player).
 */
export function useDropdownPortalPos(
  anchorRef: RefObject<HTMLDivElement | null>,
  open: boolean,
): { right: number; bottom: number } | null {
  const [pos, setPos] = useState<{ right: number; bottom: number } | null>(
    null,
  );

  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }

    let rafId: number;
    const update = () => {
      const el = anchorRef.current;
      if (el) {
        const rect = el.getBoundingClientRect();
        setPos({
          right: window.innerWidth - rect.right,
          bottom: window.innerHeight - rect.top + 4,
        });
      }
      rafId = requestAnimationFrame(update);
    };

    update();
    return () => cancelAnimationFrame(rafId);
  }, [open, anchorRef]);

  return pos;
}
