/**
 * SubtitleMenu — dropdown for subtitle track selection, download, and style settings.
 */
import { cn } from "@tokiomo/components";
import { memo, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { api, type SubtitleRecord } from "@/generated/rust-api";
import { usePlayer, useVideoTrackState } from "@/system";
import {
  groupSubtitleTracks,
  PlayerControlTooltip,
  renderSubtitleTrackLabel,
  renderSubtitleTriggerSummary,
  useDismissOnOutsidePointerDown,
  useDropdownPortalPos,
} from "../player/player-controls-shared";
import { SubtitlePickerModal } from "./SubtitlePickerModal";
import { SubtitleStyleSettingsForm } from "./SubtitleStyleSettingsForm";

export const SubtitleMenu = memo(function SubtitleMenu() {
  const { i18n } = useTranslation();
  const {
    subtitleTracks,
    activeSubtitleId,
    subtitleStyleSettings,
    setSubtitle,
    updateSubtitleStyleSettings,
    registerSubtitleTrack,
    removeSubtitleTrack,
  } = useVideoTrackState();
  const { item } = usePlayer();
  const [open, setOpen] = useState(false);
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const portalRef = useRef<HTMLDivElement | null>(null);
  const containerRef = useDismissOnOutsidePointerDown(
    open,
    () => setOpen(false),
    [".player-subtitle-select-popup"],
    [portalRef],
  );
  const portalPos = useDropdownPortalPos(containerRef, open);
  const groupedSubtitleTracks = groupSubtitleTracks(subtitleTracks);
  const activeSubtitleTrack =
    subtitleTracks.find((track) => track.id === activeSubtitleId) ?? undefined;
  const locale = i18n.resolvedLanguage ?? i18n.language ?? "zh-CN";

  const deleteMutation = api.subtitle.delete.useMutation({
    onSuccess: (_, subtitleId) => {
      removeSubtitleTrack(subtitleId);
      setDeletingId(null);
    },
    onError: () => {
      setDeletingId(null);
    },
  });

  const disableAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSubtitle(null);
    setOpen(false);
  };

  const handleDelete = (subtitleId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setDeletingId(subtitleId);
    deleteMutation.mutate(subtitleId);
  };

  useEffect(() => {
    if (!open) {
      setShowSettings(false);
    }
  }, [open]);

  return (
    <>
      <div ref={containerRef} className="relative">
        <PlayerControlTooltip title="字幕">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setOpen((o) => !o);
            }}
            className={`flex h-8 cursor-pointer items-center gap-1.5 rounded px-2 hover:bg-white/10 ${
              activeSubtitleId
                ? "text-white/80 hover:text-white"
                : "text-white/80 hover:text-white"
            }`}
          >
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M20 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 14H4V6h16v12zM6 10h2v2H6zm0 4h8v2H6zm10 0h2v2h-2zm-6-4h8v2h-8z" />
            </svg>
            {activeSubtitleTrack ? (
              <span className="min-w-0 truncate text-[11px]">
                {renderSubtitleTriggerSummary(activeSubtitleTrack)}
              </span>
            ) : null}
          </button>
        </PlayerControlTooltip>
        {open &&
          portalPos &&
          createPortal(
            <div
              ref={portalRef}
              className={cn(
                "player-popup-in fixed z-[99999] overflow-hidden rounded-lg bg-black/65 shadow-2xl ring-1 ring-white/15 backdrop-blur-2xl transition-[width]",
                showSettings ? "w-[22rem]" : "min-w-[12rem]",
              )}
              style={{ right: portalPos.right, bottom: portalPos.bottom }}
            >
              {showSettings ? (
                /* ── Settings panel ── */
                <div className="player-popup-in flex max-h-[min(32rem,80vh)] flex-col">
                  {/* Header */}
                  <div className="flex flex-shrink-0 items-center gap-2 border-b border-white/10 px-3 py-2.5">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setShowSettings(false);
                      }}
                      className="flex cursor-pointer items-center gap-1 text-xs text-white/50 hover:text-white/90"
                    >
                      <svg
                        className="h-3 w-3"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={2.5}
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          d="M15 19l-7-7 7-7"
                        />
                      </svg>
                      返回
                    </button>
                    <span className="text-xs font-medium text-white/80">
                      字幕样式
                    </span>
                  </div>

                  {/* Scrollable settings body */}
                  <div className="overflow-y-auto px-3 pt-2 pb-1">
                    <SubtitleStyleSettingsForm
                      settings={subtitleStyleSettings}
                      onChange={updateSubtitleStyleSettings}
                      variant="dark"
                    />
                  </div>
                </div>
              ) : (
                /* ── Track list ── */
                <div className="player-popup-in">
                  <button
                    type="button"
                    onClick={disableAll}
                    className={`flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-xs hover:bg-white/10 ${!activeSubtitleId ? "text-[var(--accent)]" : "text-white/90"}`}
                  >
                    <span className="w-3 flex-shrink-0">
                      {!activeSubtitleId ? "✓" : ""}
                    </span>
                    关闭
                  </button>
                  {groupedSubtitleTracks.length === 0 ? (
                    <div className="px-3 py-2 text-xs text-white/60">
                      当前没有可用字幕
                    </div>
                  ) : (
                    groupedSubtitleTracks.map((group) => (
                      <div key={group.key}>
                        <div className="border-t border-white/5 px-3 py-1.5 text-[10px] font-medium tracking-wide text-white/40 first:border-t-0">
                          {group.title}
                        </div>
                        {group.items.map((sub) => {
                          const isActive = sub.id === activeSubtitleId;
                          return (
                            <div
                              key={sub.id}
                              className="flex items-center gap-2 pr-2 text-xs text-white/90 hover:bg-white/10"
                            >
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setSubtitle(sub.id);
                                  setOpen(false);
                                }}
                                className={`flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-3 py-2 text-left ${
                                  isActive
                                    ? "text-[var(--accent)]"
                                    : "text-white/90"
                                }`}
                              >
                                <span className="w-3 flex-shrink-0">
                                  {isActive ? "✓" : ""}
                                </span>
                                <span className="min-w-0 flex-1">
                                  {renderSubtitleTrackLabel(sub, locale)}
                                </span>
                              </button>
                              {sub.sourceType === "downloaded" && (
                                <button
                                  type="button"
                                  className="flex-shrink-0 cursor-pointer text-[11px] text-red-400 hover:text-red-300 disabled:cursor-not-allowed disabled:opacity-50"
                                  disabled={deletingId === sub.id}
                                  onClick={(e) => handleDelete(sub.id, e)}
                                >
                                  {deletingId === sub.id ? "删除中" : "删除"}
                                </button>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    ))
                  )}
                  <div className="border-t border-white/10" />
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setOpen(false);
                      setIsPickerOpen(true);
                    }}
                    className="flex w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-xs text-white/90 hover:bg-white/10"
                  >
                    <span className="w-3 flex-shrink-0">↓</span>
                    搜索下载字幕
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setShowSettings(true);
                    }}
                    className="flex w-full cursor-pointer items-center gap-2 border-t border-white/10 px-3 py-2 text-left text-xs text-white/90 hover:bg-white/10"
                  >
                    <span className="w-3 flex-shrink-0">⚙</span>
                    字幕设置
                  </button>
                </div>
              )}
            </div>,
            document.body,
          )}
      </div>
      {item && (
        <SubtitlePickerModal
          open={isPickerOpen}
          onClose={() => setIsPickerOpen(false)}
          fileId={item.fileId}
          title={item.title}
          imdbId={item.imdbId}
          tmdbId={item.tmdbId}
          onSubtitleSelected={(sub: SubtitleRecord) => {
            if (!sub.storageUrl && sub.sourceType !== "embedded") return;
            registerSubtitleTrack({
              id: sub.id,
              label: sub.title || sub.language,
              language: sub.language,
              format: sub.format,
              storageUrl: sub.storageUrl ?? null,
              sourceType: sub.sourceType,
              isDefault: sub.isDefault,
              available: true,
            });
            setSubtitle(sub.id);
            setIsPickerOpen(false);
          }}
          onSubtitleDeleted={(subtitleId) => {
            removeSubtitleTrack(subtitleId);
          }}
        />
      )}
    </>
  );
});

SubtitleMenu.displayName = "SubtitleMenu";
