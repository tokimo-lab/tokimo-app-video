/**
 * FileProbePanel — Right-side detail panel for media file metadata.
 *
 * Fetches ffprobe data via the Rust API and displays format info,
 * video/audio stream details, and chapter list.
 */

import { ScrollArea } from "@tokimo/ui";
import {
  ChevronRight,
  FileText,
  Film,
  HardDrive,
  Headphones,
  Info,
  Layers,
  ListOrdered,
  Subtitles,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type FileProbeStream } from "../../api";

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / 1024 ** i;
  return `${value.toFixed(i > 0 ? 2 : 0)} ${units[i]}`;
}

function formatDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0)
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function formatBitrate(bps: number): string {
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(0)} Kbps`;
  return `${bps} bps`;
}

function formatFrameRate(rate: string): string {
  const parts = rate.split("/");
  if (parts.length === 2) {
    const num = Number(parts[0]);
    const den = Number(parts[1]);
    if (den > 0) {
      const fps = num / den;
      return `${fps % 1 === 0 ? fps : fps.toFixed(2)} fps`;
    }
  }
  return rate;
}

function langName(code: string, t: (key: string) => string): string {
  const map: Record<string, string> = {
    chi: "media.fileProbe.languages.chinese",
    zho: "media.fileProbe.languages.chinese",
    eng: "media.fileProbe.languages.english",
    jpn: "media.fileProbe.languages.japanese",
    kor: "media.fileProbe.languages.korean",
    fre: "media.fileProbe.languages.french",
    fra: "media.fileProbe.languages.french",
    ger: "media.fileProbe.languages.german",
    deu: "media.fileProbe.languages.german",
    spa: "media.fileProbe.languages.spanish",
    ita: "media.fileProbe.languages.italian",
    por: "media.fileProbe.languages.portuguese",
    rus: "media.fileProbe.languages.russian",
    und: "media.fileProbe.unknown",
  };
  const key = map[code];
  return key ? t(key) : code;
}

// ── Sub-components ───────────────────────────────────────────────────────────

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2 text-[13px]">
      <span className="shrink-0 text-fg-muted">{label}</span>
      <span className="min-w-0 break-all text-fg-primary">{value}</span>
    </div>
  );
}

function Section({
  icon,
  title,
  children,
  collapsible,
  defaultOpen = true,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
  collapsible?: boolean;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="border-t border-border-base pt-3">
      <button
        type="button"
        className="mb-2 flex w-full items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-fg-muted"
        onClick={() => collapsible && setOpen((o) => !o)}
        disabled={!collapsible}
      >
        {icon}
        {title}
        {collapsible && (
          <ChevronRight
            className={`ml-auto h-3 w-3 transition-transform ${open ? "rotate-90" : ""}`}
          />
        )}
      </button>
      {open && <div className="space-y-1.5">{children}</div>}
    </div>
  );
}

function StreamCard({
  stream,
  label,
}: {
  stream: FileProbeStream;
  label: string;
}) {
  const { t } = useTranslation();
  const tags = stream.tags;
  const lang = tags.language;
  const title = tags.title ?? tags.handler_name;

  return (
    <div className="rounded-md bg-fill-tertiary px-3 py-2 text-[13px]">
      <div className="mb-1 flex items-center gap-1.5 font-medium text-fg-primary">
        {label}
        {lang && lang !== "und" && (
          <span className="rounded bg-fill-tertiary px-1.5 py-0.5 text-[11px] font-normal">
            {langName(lang, t)}
          </span>
        )}
      </div>
      {title && <p className="mb-1 text-fg-muted">{title}</p>}
      <div className="space-y-0.5 text-fg-muted">
        <p>
          {stream.codecName}
          {stream.profile ? ` (${stream.profile})` : ""}
        </p>
        {stream.codecType === "video" && (
          <>
            {stream.width != null && stream.height != null && (
              <p>
                {stream.width}×{stream.height}
                {stream.displayAspectRatio
                  ? ` (${stream.displayAspectRatio})`
                  : ""}
              </p>
            )}
            {stream.pixFmt && (
              <p>
                {t("media.fileProbe.pixelFormat")}: {stream.pixFmt}
              </p>
            )}
            {stream.frameRate && (
              <p>
                {t("media.fileProbe.frameRate")}:{" "}
                {formatFrameRate(stream.frameRate)}
              </p>
            )}
            {stream.colorSpace && (
              <p>
                {t("media.fileProbe.colorSpace")}: {stream.colorSpace}
              </p>
            )}
            {stream.colorTransfer && (
              <p>
                {t("media.fileProbe.colorTransfer")}: {stream.colorTransfer}
              </p>
            )}
          </>
        )}
        {stream.codecType === "audio" && (
          <>
            {stream.sampleRate != null && (
              <p>
                {t("media.fileProbe.sampleRate")}:{" "}
                {stream.sampleRate.toLocaleString()} Hz
              </p>
            )}
            {stream.channels != null && (
              <p>
                {t("media.fileProbe.channels")}: {stream.channels}
                {stream.channelLayout ? ` (${stream.channelLayout})` : ""}
              </p>
            )}
          </>
        )}
        {stream.bitRate && (
          <p>
            {t("media.fileProbe.bitrate")}:{" "}
            {formatBitrate(Number(stream.bitRate))}
          </p>
        )}
      </div>
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

interface FileProbeProps {
  fileSystemId: string;
  filePath: string;
  fileName: string;
}

export function FileProbePanel({
  fileSystemId,
  filePath,
  fileName,
}: FileProbeProps) {
  const { t } = useTranslation();
  const { data, isLoading, error } = api.vfs.probe.useQuery(
    { fileSystemId, path: filePath },
    { enabled: !!fileSystemId && !!filePath, staleTime: 5 * 60_000 },
  );

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        {t("media.fileProbe.probing")}
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        <Info className="mr-1.5 h-4 w-4" />
        {t("media.fileProbe.failed")}
      </div>
    );
  }

  const fmt = data.format;
  const videoStreams = data.streams.filter((s) => s.codecType === "video");
  const audioStreams = data.streams.filter((s) => s.codecType === "audio");
  const subtitleStreams = data.streams.filter(
    (s) => s.codecType === "subtitle",
  );

  const primaryVideo = videoStreams[0];

  return (
    <ScrollArea
      className="h-full"
      direction="vertical"
      innerClassName="px-4 py-3"
    >
      {/* File name */}
      <h3 className="mb-3 break-all text-sm font-semibold text-fg-primary">
        {fileName}
      </h3>

      <div className="space-y-3">
        {/* ── Format section ──────────────────────────────────────── */}
        <Section
          icon={<FileText className="h-3 w-3" />}
          title={t("media.fileProbe.format")}
        >
          <InfoRow
            label={t("media.fileProbe.container")}
            value={fmt.formatName}
          />
          {fmt.formatLongName && fmt.formatLongName !== fmt.formatName && (
            <InfoRow
              label={t("media.fileProbe.fullName")}
              value={fmt.formatLongName}
            />
          )}
          {fmt.duration != null && (
            <InfoRow
              label={t("media.fileProbe.duration")}
              value={formatDuration(fmt.duration)}
            />
          )}
          {fmt.size != null && (
            <InfoRow
              label={t("media.fileProbe.fileSize")}
              value={`${formatBytes(fmt.size)} (${fmt.size.toLocaleString()} bytes)`}
            />
          )}
          {fmt.bitRate != null && (
            <InfoRow
              label={t("media.fileProbe.totalBitrate")}
              value={formatBitrate(fmt.bitRate)}
            />
          )}
          <InfoRow
            label={t("media.fileProbe.streamCount")}
            value={String(fmt.nbStreams)}
          />
        </Section>

        {/* ── Video section ───────────────────────────────────────── */}
        {videoStreams.length > 0 && (
          <Section
            icon={<Film className="h-3 w-3" />}
            title={t("media.fileProbe.video")}
          >
            {primaryVideo &&
              primaryVideo.width != null &&
              primaryVideo.height != null && (
                <div className="mb-2 text-center text-lg font-bold text-fg-primary">
                  {primaryVideo.width}×{primaryVideo.height}
                </div>
              )}
            {videoStreams.map((s, i) => (
              <StreamCard
                key={s.index}
                stream={s}
                label={
                  videoStreams.length > 1
                    ? t("media.fileProbe.videoNumber", { number: i + 1 })
                    : t("media.fileProbe.video")
                }
              />
            ))}
          </Section>
        )}

        {/* ── Audio section ───────────────────────────────────────── */}
        {audioStreams.length > 0 && (
          <Section
            icon={<Headphones className="h-3 w-3" />}
            title={t("media.fileProbe.audio")}
          >
            {audioStreams.map((s, i) => (
              <StreamCard
                key={s.index}
                stream={s}
                label={
                  audioStreams.length > 1
                    ? t("media.fileProbe.audioNumber", { number: i + 1 })
                    : t("media.fileProbe.audio")
                }
              />
            ))}
          </Section>
        )}

        {/* ── Subtitle section ────────────────────────────────────── */}
        {subtitleStreams.length > 0 && (
          <Section
            icon={<Subtitles className="h-3 w-3" />}
            title={t("media.fileProbe.subtitles", {
              count: subtitleStreams.length,
            })}
            collapsible
            defaultOpen={false}
          >
            {subtitleStreams.map((s, i) => {
              const lang = s.tags.language;
              const title = s.tags.title;
              return (
                <div
                  key={s.index}
                  className="rounded-md bg-fill-tertiary px-3 py-1.5 text-[13px]"
                >
                  <span className="font-medium text-fg-primary">
                    #{i + 1} {s.codecName}
                  </span>
                  {lang && lang !== "und" && (
                    <span className="ml-1.5 rounded bg-fill-tertiary px-1.5 py-0.5 text-[11px]">
                      {langName(lang, t)}
                    </span>
                  )}
                  {title && (
                    <span className="ml-1.5 text-fg-muted">{title}</span>
                  )}
                </div>
              );
            })}
          </Section>
        )}

        {/* ── Chapters section ────────────────────────────────────── */}
        {data.chapters.length > 0 && (
          <Section
            icon={<ListOrdered className="h-3 w-3" />}
            title={t("media.fileProbe.chapters", {
              count: data.chapters.length,
            })}
            collapsible
            defaultOpen={false}
          >
            {data.chapters.map((ch) => (
              <div
                key={ch.id}
                className="flex items-baseline gap-2 text-[13px]"
              >
                <span className="shrink-0 font-mono text-fg-muted">
                  {formatDuration(Number(ch.startTime))}
                </span>
                <span className="text-fg-primary">
                  {ch.title ?? `Chapter ${ch.id + 1}`}
                </span>
              </div>
            ))}
          </Section>
        )}

        {/* ── Tags section ────────────────────────────────────────── */}
        {Object.keys(fmt.tags).length > 0 && (
          <Section
            icon={<Layers className="h-3 w-3" />}
            title={t("media.fileProbe.tags")}
            collapsible
            defaultOpen={false}
          >
            {Object.entries(fmt.tags).map(([k, v]) => (
              <InfoRow key={k} label={k} value={v} />
            ))}
          </Section>
        )}

        {/* ── Path info ──────────────────────────────────────────── */}
        <Section
          icon={<HardDrive className="h-3 w-3" />}
          title={t("media.fileProbe.path")}
        >
          <p className="break-all text-[13px] text-fg-muted">{filePath}</p>
        </Section>
      </div>
    </ScrollArea>
  );
}
