/**
 * SubtitleStyleSettingsForm — 字幕样式设置表单
 *
 * 支持两种外观变体：
 * - "dark"  : 播放器内悬浮面板（深色背景，白色文字）
 * - "light" : 个人设置弹窗内（标准亮/暗主题）
 */
import { cn, Select, Slider } from "@tokiomo/components";
import type {
  SubtitlePosition,
  SubtitleRenderMode,
  SubtitleStyleSettings,
  SubtitleTextShadow,
} from "@/lib/player-subtitles";
import { getTextShadowCss } from "@/lib/player-subtitles";

export const SUBTITLE_POSITION_OPTIONS = [
  { label: "底部", value: "bottom" },
  { label: "中间", value: "middle" },
  { label: "顶部", value: "top" },
] as const;

export const SUBTITLE_FONT_OPTIONS = [
  { label: "系统默认", value: "system-ui, sans-serif" },
  { label: "Arial", value: "Arial, sans-serif" },
  {
    label: "中文无衬线",
    value: '"PingFang SC", "Microsoft YaHei", sans-serif',
  },
  { label: "衬线", value: "Georgia, serif" },
  { label: "等宽", value: '"SFMono-Regular", Consolas, monospace' },
] as const;

export const SUBTITLE_SHADOW_OPTIONS = [
  { value: "none", label: "无", css: "none" },
  {
    value: "dropshadow",
    label: "投影",
    css: "0 2px 4px rgba(0,0,0,0.95), 0 1px 2px rgba(0,0,0,0.85)",
  },
  {
    value: "raised",
    label: "浮雕",
    css: "0 1px 0 rgba(0,0,0,0.9), 0 2px 0 rgba(0,0,0,0.7), 0 3px 0 rgba(0,0,0,0.5)",
  },
  {
    value: "uniform",
    label: "轮廓",
    css: "-1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000",
  },
] as const;

export const RENDER_MODE_OPTIONS = [
  { value: "auto", label: "自动", desc: "文本原生渲染，ASS/PGS 用专用引擎" },
  { value: "native", label: "原生", desc: "浏览器渲染，ASS 样式简化为纯文本" },
  { value: "custom", label: "自定义", desc: "JS 渲染，完整支持背景色等样式" },
] as const;

interface SubtitleStyleSettingsFormProps {
  settings: SubtitleStyleSettings;
  onChange: (partial: Partial<SubtitleStyleSettings>) => void;
  variant?: "dark" | "light";
}

export function SubtitleStyleSettingsForm({
  settings,
  onChange,
  variant = "dark",
}: SubtitleStyleSettingsFormProps) {
  const isLight = variant === "light";

  const isTransparentBg =
    settings.backgroundColor === "rgba(0,0,0,0)" ||
    settings.backgroundColor === "transparent" ||
    settings.backgroundColor === "";

  const bgColorHex = isTransparentBg
    ? "#000000"
    : settings.backgroundColor.startsWith("#")
      ? settings.backgroundColor
      : "#000000";

  // ── style helpers ──
  const sectionLabel = cn(
    "text-[10px] font-medium uppercase tracking-wider",
    isLight ? "text-fg-muted" : "text-white/35",
  );

  const fieldLabel = cn("text-xs", isLight ? "text-fg-muted" : "text-white/55");

  const segmentBase = cn(
    "flex overflow-hidden rounded-md border",
    isLight ? "border-border-base dark:border-white/10" : "border-white/10",
  );

  const segmentBtn = (active: boolean) =>
    cn(
      "flex-1 py-1.5 text-center text-xs transition-colors",
      isLight
        ? active
          ? "bg-[var(--accent)]/10 text-[var(--accent)] font-medium"
          : "text-fg-muted hover:bg-gray-100 dark:hover:bg-white/5"
        : active
          ? "bg-white/20 text-white"
          : "text-white/55 hover:bg-white/10 hover:text-white/85",
    );

  const colorRowItem = cn(
    "flex items-center gap-2 rounded border px-2 py-1.5",
    isLight
      ? "border-border-base dark:border-white/10 bg-surface-base dark:bg-white/[0.04] hover:bg-fill-tertiary"
      : "border-white/10 bg-white/[0.04] hover:bg-white/[0.07]",
  );

  const divider = cn(
    "border-t",
    isLight
      ? "border-border-subtle dark:border-white/[0.06] mx-0"
      : "border-white/[0.06] mx-3",
  );

  return (
    <div
      className={cn("text-xs", isLight ? "text-fg-secondary" : "text-white/85")}
    >
      {/* ── 渲染模式 ── */}
      <div className={cn("pb-2.5", isLight ? "pt-0" : "px-3 pt-3")}>
        <div className={cn("mb-2", sectionLabel)}>渲染模式</div>
        <div className={segmentBase}>
          {RENDER_MODE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() =>
                onChange({ renderMode: opt.value as SubtitleRenderMode })
              }
              className={segmentBtn(settings.renderMode === opt.value)}
            >
              {opt.label}
            </button>
          ))}
        </div>
        <p
          className={cn(
            "mt-1.5 text-[10px] leading-relaxed",
            isLight ? "text-fg-muted" : "text-white/30",
          )}
        >
          {
            RENDER_MODE_OPTIONS.find((o) => o.value === settings.renderMode)
              ?.desc
          }
        </p>
      </div>

      <div className={divider} />

      {/* ── 外观 ── */}
      <div className={cn("py-2.5 space-y-2.5", isLight ? "" : "px-3")}>
        <div className={sectionLabel}>外观</div>

        {/* Color row */}
        <div className="grid grid-cols-2 gap-2">
          {/* Font color */}
          <div className="flex items-center gap-2">
            <span className={cn("w-7 flex-shrink-0", fieldLabel)}>文字</span>
            <label className={cn("flex flex-1 cursor-pointer", colorRowItem)}>
              <input
                type="color"
                value={settings.color}
                onChange={(e) => onChange({ color: e.target.value })}
                className="h-3.5 w-3.5 cursor-pointer rounded-sm border-0 bg-transparent p-0 flex-shrink-0"
                aria-label="字幕颜色"
              />
              <span
                className={cn(
                  "font-mono text-[10px]",
                  isLight ? "text-fg-muted" : "text-white/45",
                )}
              >
                {settings.color.toUpperCase()}
              </span>
            </label>
          </div>

          {/* Background color */}
          <div className="flex items-center gap-2">
            <span className={cn("w-7 flex-shrink-0", fieldLabel)}>背景</span>
            <div className={cn("flex flex-1 items-center gap-1", colorRowItem)}>
              {isTransparentBg ? (
                <button
                  type="button"
                  onClick={() => onChange({ backgroundColor: "#000000" })}
                  className={cn(
                    "flex cursor-pointer items-center gap-1.5",
                    isLight
                      ? "text-fg-muted hover:text-fg-secondary"
                      : "text-white/40 hover:text-white/70",
                  )}
                  title="点击选择背景色"
                >
                  <span
                    className="inline-block h-3.5 w-3.5 flex-shrink-0 rounded-sm border border-border-base dark:border-white/20"
                    style={{
                      background:
                        "repeating-linear-gradient(45deg,#aaa 0,#aaa 1px,transparent 0,transparent 50%) 0 0 / 4px 4px",
                    }}
                  />
                  <span className="text-[10px]">透明</span>
                </button>
              ) : (
                <>
                  <label className="flex cursor-pointer items-center gap-1.5">
                    <input
                      type="color"
                      value={bgColorHex}
                      onChange={(e) =>
                        onChange({ backgroundColor: e.target.value })
                      }
                      className="h-3.5 w-3.5 cursor-pointer rounded-sm border-0 bg-transparent p-0"
                      aria-label="字幕背景色"
                    />
                    <span
                      className={cn(
                        "font-mono text-[10px]",
                        isLight ? "text-fg-muted" : "text-white/45",
                      )}
                    >
                      {bgColorHex.toUpperCase()}
                    </span>
                  </label>
                  <button
                    type="button"
                    onClick={() =>
                      onChange({ backgroundColor: "rgba(0,0,0,0)" })
                    }
                    className={cn(
                      "ml-auto text-[10px]",
                      isLight
                        ? "text-fg-secondary hover:text-fg-secondary"
                        : "text-white/30 hover:text-white/60",
                    )}
                    title="清除背景色"
                  >
                    ✕
                  </button>
                </>
              )}
            </div>
          </div>
        </div>

        {/* Shadow / Outline */}
        <div>
          <div className={cn("mb-2", fieldLabel)}>阴影 / 轮廓</div>
          <div className="grid grid-cols-4 gap-1.5">
            {SUBTITLE_SHADOW_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                onClick={() =>
                  onChange({ textShadow: opt.value as SubtitleTextShadow })
                }
                className={cn(
                  "flex flex-col items-center gap-1 rounded-md py-2 transition-colors",
                  isLight
                    ? settings.textShadow === opt.value
                      ? "bg-[var(--accent)]/10 ring-1 ring-[var(--accent)]/30"
                      : "bg-fill-tertiary dark:bg-white/[0.04] hover:bg-fill-tertiary"
                    : settings.textShadow === opt.value
                      ? "bg-white/20 ring-1 ring-white/25"
                      : "bg-white/[0.04] hover:bg-white/10",
                )}
              >
                <span
                  className={cn(
                    "text-sm font-bold leading-none",
                    isLight ? "text-gray-800 dark:text-white" : "text-white",
                  )}
                  style={{
                    textShadow: getTextShadowCss(
                      opt.value as SubtitleTextShadow,
                    ),
                  }}
                >
                  A
                </span>
                <span
                  className={cn(
                    "text-[9px]",
                    isLight ? "text-fg-muted" : "text-white/45",
                  )}
                >
                  {opt.label}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Font weight */}
        <div className="flex items-center justify-between">
          <span className={fieldLabel}>粗细</span>
          <div
            className={cn(
              "flex overflow-hidden rounded border",
              isLight
                ? "border-border-base dark:border-white/10"
                : "border-white/10",
            )}
          >
            <button
              type="button"
              onClick={() => onChange({ fontWeight: "normal" })}
              className={cn(
                "px-3 py-1 text-xs transition-colors",
                isLight
                  ? settings.fontWeight === "normal"
                    ? "bg-[var(--accent)]/10 text-[var(--accent)]"
                    : "text-fg-muted hover:bg-gray-100 dark:hover:bg-white/5"
                  : settings.fontWeight === "normal"
                    ? "bg-white/20 text-white"
                    : "text-white/45 hover:bg-white/10 hover:text-white/80",
              )}
            >
              常规
            </button>
            <button
              type="button"
              onClick={() => onChange({ fontWeight: "bold" })}
              className={cn(
                "px-3 py-1 text-xs font-bold transition-colors",
                isLight
                  ? settings.fontWeight === "bold"
                    ? "bg-[var(--accent)]/10 text-[var(--accent)]"
                    : "text-fg-muted hover:bg-gray-100 dark:hover:bg-white/5"
                  : settings.fontWeight === "bold"
                    ? "bg-white/20 text-white"
                    : "text-white/45 hover:bg-white/10 hover:text-white/80",
              )}
            >
              粗体
            </button>
          </div>
        </div>
      </div>

      <div className={divider} />

      {/* ── 文字 ── */}
      <div className={cn("space-y-2.5 py-2.5", isLight ? "" : "px-3")}>
        <div className={sectionLabel}>文字</div>

        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <span className={fieldLabel}>大小</span>
            <span
              className={cn(
                "font-mono text-[10px]",
                isLight ? "text-fg-muted" : "text-white/35",
              )}
            >
              {settings.fontSize}px
            </span>
          </div>
          <Slider
            min={16}
            max={60}
            step={2}
            value={settings.fontSize}
            onChange={(v) => onChange({ fontSize: v })}
            className="w-full"
            aria-label="字幕大小"
          />
          <div
            className={cn(
              "mt-0.5 flex justify-between text-[9px]",
              isLight ? "text-fg-muted" : "text-white/20",
            )}
          >
            <span>16px</span>
            <span>60px</span>
          </div>
        </div>

        <div>
          <div className={cn("mb-1.5", fieldLabel)}>字体</div>
          <Select
            options={SUBTITLE_FONT_OPTIONS.map((option) => ({
              label: option.label,
              value: option.value,
            }))}
            value={settings.fontFamily}
            onChange={(value) => onChange({ fontFamily: value as string })}
            size="small"
            className={cn(
              "w-full",
              isLight ? "" : "border-white/10 bg-white/5 text-white",
            )}
            popupClassName={isLight ? "" : "player-subtitle-select-popup"}
          />
        </div>
      </div>

      <div className={divider} />

      {/* ── 位置 ── */}
      <div className={cn("py-2.5", isLight ? "" : "px-3")}>
        <div className={cn("mb-2", sectionLabel)}>位置</div>
        <div className={segmentBase}>
          {SUBTITLE_POSITION_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() =>
                onChange({ position: opt.value as SubtitlePosition })
              }
              className={segmentBtn(settings.position === opt.value)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <p
        className={cn(
          "text-[10px] leading-relaxed",
          isLight ? "text-fg-muted mt-1" : "px-3 pb-3 text-white/30",
        )}
      >
        文本字幕即时应用样式；ASS/PGS 字幕在自定义渲染模式下可能保留原始样式。
      </p>

      {/* ── 预览 ── */}
      {isLight && (
        <div
          className="mt-4 rounded-lg bg-gray-900 dark:bg-black/60 p-4 flex items-end justify-center"
          style={{ minHeight: 80 }}
        >
          <span
            style={{
              color: settings.color,
              backgroundColor: isTransparentBg
                ? "transparent"
                : settings.backgroundColor,
              fontSize: Math.round(settings.fontSize * 0.55),
              fontFamily: settings.fontFamily,
              fontWeight: settings.fontWeight,
              textShadow: getTextShadowCss(settings.textShadow),
              padding: isTransparentBg ? "0 4px" : "2px 8px",
              borderRadius: 4,
            }}
          >
            这是字幕预览效果
          </span>
        </div>
      )}
    </div>
  );
}
