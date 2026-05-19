import { cn } from "@tokimo/ui";
import * as LucideIcons from "lucide-react";
import type { ComponentType } from "react";

const HASH_PALETTE = [
  "#ef4444",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#14b8a6",
  "#06b6d4",
  "#3b82f6",
  "#6366f1",
  "#a855f7",
  "#ec4899",
];

function hashColor(str: string): string {
  let h = 0;
  for (let i = 0; i < str.length; i++) {
    h = str.charCodeAt(i) + ((h << 5) - h);
  }
  return HASH_PALETTE[Math.abs(h) % HASH_PALETTE.length]!;
}

function kebabToPascal(name: string): string {
  return name
    .split("-")
    .map((p) => (p.length === 0 ? p : p[0]!.toUpperCase() + p.slice(1)))
    .join("");
}

function isLucideIcon(icon: string | undefined | null): boolean {
  return !!icon?.startsWith("lucide:");
}

type LucideComponent = ComponentType<{
  className?: string;
  style?: React.CSSProperties;
}>;

const lucideRegistry = LucideIcons as unknown as Record<
  string,
  LucideComponent
>;

function resolveLucideIcon(
  icon: string | undefined | null,
): LucideComponent | null {
  if (!isLucideIcon(icon)) return null;
  const name = kebabToPascal(icon!.slice(7));
  return lucideRegistry[name] ?? null;
}

/**
 * Video-app local AppIcon. Mirrors the host shell's component but resolves
 * lucide icons by transforming the kebab-case suffix to PascalCase and
 * looking it up in `lucide-react`, instead of depending on the host's
 * 600+ entry `icon-catalog`.
 */
export function AppIcon({
  icon,
  iconComponent: IconComponent,
  image,
  color,
  size = 40,
  surface,
  className,
  onClick,
}: {
  icon?: string | null;
  iconComponent?: LucideComponent;
  image?: string | null;
  color?: string | null;
  size?: number;
  surface?: "neutral";
  className?: string;
  onClick?: () => void;
}) {
  const ResolvedIcon = IconComponent ?? resolveLucideIcon(icon);
  const hasEmoji = !image && !IconComponent && !isLucideIcon(icon) && !!icon;
  const isNeutral = surface === "neutral";

  const bgColor = image
    ? undefined
    : isNeutral
      ? undefined
      : color === "transparent"
        ? undefined
        : color || (hasEmoji ? hashColor(icon!) : undefined);
  const neutralSurfaceClass =
    isNeutral && !image ? "bg-black/[0.05] dark:bg-white/[0.06]" : "";

  const lucideScale = size <= 24 ? 0.6 : 0.45;
  const textScale = size <= 24 ? 0.85 : 0.45;

  const iconColorClass = bgColor
    ? "text-white/90"
    : isNeutral && color
      ? undefined
      : "text-fg-muted";
  const iconInlineColor =
    isNeutral && color && color !== "transparent" ? color : undefined;

  const content = image ? (
    <img
      src={image}
      alt=""
      width={size}
      height={size}
      className="object-cover"
      style={{ width: size, height: size }}
      draggable={false}
    />
  ) : ResolvedIcon ? (
    <ResolvedIcon
      className={iconColorClass}
      style={{
        width: size * lucideScale,
        height: size * lucideScale,
        color: iconInlineColor,
      }}
    />
  ) : hasEmoji ? (
    <span
      className={cn("text-center leading-none", bgColor && "text-white")}
      style={{ fontSize: size * textScale, whiteSpace: "nowrap" }}
    >
      {icon}
    </span>
  ) : null;

  const baseClass =
    "rounded-[20%] flex items-center justify-center select-none shrink-0 overflow-hidden";

  if (onClick) {
    return (
      <button
        type="button"
        data-app-icon
        className={cn(
          baseClass,
          neutralSurfaceClass,
          "cursor-pointer hover:ring-4 hover:ring-black/10 dark:hover:ring-white/10 transition-all",
          className,
        )}
        style={{ width: size, height: size, backgroundColor: bgColor }}
        onClick={onClick}
      >
        {content}
      </button>
    );
  }

  return (
    <div
      data-app-icon
      className={cn(baseClass, neutralSurfaceClass, className)}
      style={{ width: size, height: size, backgroundColor: bgColor }}
    >
      {content}
    </div>
  );
}
