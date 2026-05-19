import { AppSidebar, CircularProgress, Tooltip } from "@tokimo/ui";
import { PanelLeft, PanelLeftClose, Plus, Settings } from "lucide-react";
import type { VideoOutput } from "./shell-shim/api";
import { getAvatarColor, getAvatarIcon } from "../shell-shim/shared";
import { AppIcon } from "../shell-shim/shared";

export default function VideoSidebar({
  categories,
  activeId,
  onSelect,
  collapsed,
  onCreateClick,
  onSettingsClick,
  syncProgress,
  onToggleCollapse,
  settingsActive = false,
}: {
  categories: VideoOutput[];
  activeId: string | null;
  onSelect: (id: string) => void;
  collapsed?: boolean;
  onCreateClick: () => void;
  onSettingsClick: () => void;
  syncProgress?: Record<string, { isActive: boolean; pct: number }>;
  onToggleCollapse?: () => void;
  /** When true, the settings (⚙) button shows a highlighted state. */
  settingsActive?: boolean;
}) {
  const sections = [
    {
      items: categories.map((cat) => {
        const sp = syncProgress?.[cat.id];
        return {
          key: cat.id,
          icon: (
            <AppIcon
              icon={getAvatarIcon(cat.avatar) || cat.name}
              color={getAvatarColor(cat.avatar)}
              size={24}
            />
          ),
          collapsedIcon: sp?.isActive ? (
            <span className="relative flex h-8 w-8 items-center justify-center">
              <AppIcon
                icon={getAvatarIcon(cat.avatar)}
                color={getAvatarColor(cat.avatar)}
                size={24}
              />
              <CircularProgress
                value={sp.pct}
                size={32}
                strokeWidth={2}
                showText={false}
                className="absolute left-0 top-0"
              />
            </span>
          ) : undefined,
          label: cat.name,
          extra: (() => {
            if (sp?.isActive) {
              return <CircularProgress value={sp.pct} size={24} />;
            }
            if (collapsed) return undefined;
            return cat.itemCount > 0 ? (
              <span className="text-[10px] tabular-nums text-fg-muted">
                {cat.itemCount}
              </span>
            ) : undefined;
          })(),
        };
      }),
    },
  ];

  const collapsedFooter = (
    <div className="flex flex-col items-center gap-1">
      <Tooltip title="新建视频库" placement="right">
        <button
          type="button"
          onClick={onCreateClick}
          className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg text-fg-muted transition-all hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
        >
          <Plus className="h-4 w-4" />
        </button>
      </Tooltip>
      <Tooltip title="视频库设置" placement="right">
        <button
          type="button"
          onClick={onSettingsClick}
          className={`flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg transition-all ${
            settingsActive
              ? "bg-black/[0.08] text-fg-primary dark:bg-white/[0.08]"
              : "text-fg-muted hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
          }`}
        >
          <Settings className="h-4 w-4" />
        </button>
      </Tooltip>
      <Tooltip title="展开侧边栏" placement="right">
        <button
          type="button"
          onClick={onToggleCollapse}
          className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg text-fg-muted transition-all hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
        >
          <PanelLeft className="h-4 w-4" />
        </button>
      </Tooltip>
    </div>
  );

  const fullFooter = (
    <div className="flex items-center gap-1">
      <Tooltip title="新建视频库">
        <button
          type="button"
          onClick={onCreateClick}
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-fg-muted transition-all hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
        >
          <Plus className="h-4 w-4" />
        </button>
      </Tooltip>
      <Tooltip title="视频库设置">
        <button
          type="button"
          onClick={onSettingsClick}
          className={`flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg transition-all ${
            settingsActive
              ? "bg-black/[0.08] text-fg-primary dark:bg-white/[0.08]"
              : "text-fg-muted hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
          }`}
        >
          <Settings className="h-4 w-4" />
        </button>
      </Tooltip>
      <Tooltip title="收起侧边栏">
        <button
          type="button"
          onClick={onToggleCollapse}
          className="ml-auto flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg text-fg-muted transition-all hover:bg-black/[0.08] hover:text-fg-secondary dark:hover:bg-white/[0.08]"
        >
          <PanelLeftClose className="h-4 w-4" />
        </button>
      </Tooltip>
    </div>
  );

  return (
    <AppSidebar
      sections={sections}
      activeKey={activeId ?? undefined}
      onSelect={onSelect}
      collapsed={collapsed}
      footer={collapsed ? collapsedFooter : fullFooter}
    />
  );
}
