/**
 * FileProbePanel — Right-side detail panel for media file metadata.
 *
 * Fetches ffprobe data via the Rust API and displays format info,
 * video/audio stream details, and chapter list.
 */

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
import { api, type FileProbeStream } from "@/generated/rust-api";

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

function langName(code: string): string {
  const map: Record<string, string> = {
    chi: "中文",
    zho: "中文",
    eng: "英语",
    jpn: "日语",
    kor: "韩语",
    fre: "法语",
    fra: "法语",
    ger: "德语",
    deu: "德语",
    spa: "西班牙语",
    ita: "意大利语",
    por: "葡萄牙语",
    rus: "俄语",
    und: "未知",
  };
  return map[code] ?? code;
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
  const tags = stream.tags;
  const lang = tags.language;
  const title = tags.title ?? tags.handler_name;

  return (
    <div className="rounded-md bg-fill-tertiary px-3 py-2 text-[13px]">
      <div className="mb-1 flex items-center gap-1.5 font-medium text-fg-primary">
        {label}
        {lang && lang !== "und" && (
          <span className="rounded bg-fill-tertiary px-1.5 py-0.5 text-[11px] font-normal">
            {langName(lang)}
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
            {stream.pixFmt && <p>像素格式: {stream.pixFmt}</p>}
            {stream.frameRate && (
              <p>帧率: {formatFrameRate(stream.frameRate)}</p>
            )}
            {stream.colorSpace && <p>色彩空间: {stream.colorSpace}</p>}
            {stream.colorTransfer && <p>传输特性: {stream.colorTransfer}</p>}
          </>
        )}
        {stream.codecType === "audio" && (
          <>
            {stream.sampleRate != null && (
              <p>采样率: {stream.sampleRate.toLocaleString()} Hz</p>
            )}
            {stream.channels != null && (
              <p>
                声道: {stream.channels}
                {stream.channelLayout ? ` (${stream.channelLayout})` : ""}
              </p>
            )}
          </>
        )}
        {stream.bitRate && <p>码率: {formatBitrate(Number(stream.bitRate))}</p>}
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
  const { data, isLoading, error } = api.vfs.probe.useQuery(
    { fileSystemId, path: filePath },
    { enabled: !!fileSystemId && !!filePath, staleTime: 5 * 60_000 },
  );

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        探测中…
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        <Info className="mr-1.5 h-4 w-4" />
        无法获取文件信息
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
    <div className="custom-scrollbar h-full overflow-y-auto px-4 py-3">
      {/* File name */}
      <h3 className="mb-3 break-all text-sm font-semibold text-fg-primary">
        {fileName}
      </h3>

      <div className="space-y-3">
        {/* ── Format section ──────────────────────────────────────── */}
        <Section icon={<FileText className="h-3 w-3" />} title="格式">
          <InfoRow label="容器" value={fmt.formatName} />
          {fmt.formatLongName && fmt.formatLongName !== fmt.formatName && (
            <InfoRow label="全称" value={fmt.formatLongName} />
          )}
          {fmt.duration != null && (
            <InfoRow label="时长" value={formatDuration(fmt.duration)} />
          )}
          {fmt.size != null && (
            <InfoRow
              label="文件大小"
              value={`${formatBytes(fmt.size)} (${fmt.size.toLocaleString()} bytes)`}
            />
          )}
          {fmt.bitRate != null && (
            <InfoRow label="总码率" value={formatBitrate(fmt.bitRate)} />
          )}
          <InfoRow label="流数量" value={String(fmt.nbStreams)} />
        </Section>

        {/* ── Video section ───────────────────────────────────────── */}
        {videoStreams.length > 0 && (
          <Section icon={<Film className="h-3 w-3" />} title="视频">
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
                label={videoStreams.length > 1 ? `视频 #${i + 1}` : "视频"}
              />
            ))}
          </Section>
        )}

        {/* ── Audio section ───────────────────────────────────────── */}
        {audioStreams.length > 0 && (
          <Section icon={<Headphones className="h-3 w-3" />} title="音频">
            {audioStreams.map((s, i) => (
              <StreamCard
                key={s.index}
                stream={s}
                label={audioStreams.length > 1 ? `音频 #${i + 1}` : "音频"}
              />
            ))}
          </Section>
        )}

        {/* ── Subtitle section ────────────────────────────────────── */}
        {subtitleStreams.length > 0 && (
          <Section
            icon={<Subtitles className="h-3 w-3" />}
            title={`字幕 (${subtitleStreams.length})`}
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
                      {langName(lang)}
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
            title={`章节 (${data.chapters.length})`}
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
            title="标签"
            collapsible
            defaultOpen={false}
          >
            {Object.entries(fmt.tags).map(([k, v]) => (
              <InfoRow key={k} label={k} value={v} />
            ))}
          </Section>
        )}

        {/* ── Path info ──────────────────────────────────────────── */}
        <Section icon={<HardDrive className="h-3 w-3" />} title="路径">
          <p className="break-all text-[13px] text-fg-muted">{filePath}</p>
        </Section>
      </div>
    </div>
  );
}
