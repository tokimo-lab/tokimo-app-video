import { cn } from "@tokimo/ui";
import { Check } from "lucide-react";
import type { VideoTypeInfo } from "./video-types";
import { VIDEO_TYPES } from "./video-types";

function VideoTypeCard({
  info,
  selected,
  onClick,
}: {
  info: VideoTypeInfo;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group relative flex flex-col items-center gap-2 rounded-2xl border-2 p-3.5 text-center transition-all duration-150 cursor-pointer",
        selected
          ? "border-[var(--accent)] bg-[var(--accent)]/8 shadow-sm"
          : "border-transparent bg-fill-secondary hover:border-[var(--accent)]/40 hover:bg-fill-tertiary",
      )}
    >
      {selected && (
        <span className="absolute top-2 right-2 flex h-4 w-4 items-center justify-center rounded-full bg-[var(--accent)]">
          <Check className="h-2.5 w-2.5 text-white" aria-hidden />
        </span>
      )}
      <div
        className={cn(
          "flex h-11 w-11 items-center justify-center rounded-xl transition-transform group-hover:scale-105",
          info.bgClass,
        )}
      >
        <info.icon
          className={cn("h-[22px] w-[22px]", info.textClass)}
          aria-hidden
        />
      </div>
      <p
        className={cn(
          "text-xs font-semibold leading-tight",
          selected ? "text-[var(--accent-text)]" : "text-fg-primary",
        )}
      >
        {info.label}
      </p>
      <p className="line-clamp-2 text-[10px] leading-tight text-fg-muted">
        {info.description}
      </p>
    </button>
  );
}

export default function VideoTypeSelector({
  value,
  onChange,
  hideAdult,
}: {
  value?: string;
  onChange: (type: string) => void;
  hideAdult?: boolean;
}) {
  const types = hideAdult
    ? VIDEO_TYPES.filter((t) => t.type !== "adult")
    : VIDEO_TYPES;

  return (
    <div className="grid grid-cols-3 gap-2">
      {types.map((info) => (
        <VideoTypeCard
          key={info.type}
          info={info}
          selected={value === info.type}
          onClick={() => onChange(info.type)}
        />
      ))}
    </div>
  );
}
