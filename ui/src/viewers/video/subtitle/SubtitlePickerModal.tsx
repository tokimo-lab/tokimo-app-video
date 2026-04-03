import { useQueryClient } from "@tanstack/react-query";
import { Modal, Spin, Tabs, Tag } from "@tokiomo/components";
import { useEffect, useRef, useState } from "react";
import {
  api,
  type SubtitleRecord,
  type SubtitleSearchResult,
} from "@/generated/rust-api";

interface SubtitlePickerModalProps {
  open: boolean;
  onClose: () => void;
  fileId: string;
  imdbId?: string | null;
  tmdbId?: string | null;
  title?: string;
  /** Called when user selects/downloads a subtitle */
  onSubtitleSelected?: (sub: SubtitleRecord) => void;
  onSubtitleDeleted?: (subtitleId: string) => void;
}

type TabKey = "existing" | "search";

const LANG_NAMES: Record<string, string> = {
  "zh-CN": "简体中文",
  "zh-TW": "繁体中文",
  zh: "中文",
  en: "英语",
  ja: "日语",
  ko: "韩语",
  fr: "法语",
  de: "德语",
  es: "西班牙语",
};

function langLabel(code: string): string {
  return LANG_NAMES[code] ?? code;
}

function subtitleGroupLabel(sourceType?: string): string {
  if (sourceType === "embedded") return "内置字幕";
  if (sourceType === "downloaded") return "已下载字幕";
  if (sourceType === "external") return "外挂字幕";
  return "其他字幕";
}

function groupSubtitles<
  T extends {
    id: string;
    sourceType: string;
  },
>(subtitles: T[]): Array<{ key: string; title: string; items: T[] }> {
  const groupOrder = ["embedded", "external", "downloaded", "other"] as const;
  const grouped = new Map<string, T[]>();

  subtitles.forEach((subtitle) => {
    const key =
      subtitle.sourceType === "embedded" ||
      subtitle.sourceType === "external" ||
      subtitle.sourceType === "downloaded"
        ? subtitle.sourceType
        : "other";
    const items = grouped.get(key) ?? [];
    items.push(subtitle);
    grouped.set(key, items);
  });

  const result: Array<{ key: string; title: string; items: T[] }> = [];

  groupOrder.forEach((key) => {
    const items = grouped.get(key);
    if (!items?.length) {
      return;
    }
    result.push({
      key,
      title: subtitleGroupLabel(key),
      items,
    });
  });

  return result;
}

export function SubtitlePickerModal({
  open,
  onClose,
  fileId,
  imdbId,
  tmdbId,
  title,
  onSubtitleSelected,
  onSubtitleDeleted,
}: SubtitlePickerModalProps) {
  const [activeTab, setActiveTab] = useState<TabKey>("existing");
  const [searchQuery, setSearchQuery] = useState(title ?? "");
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const queryClient = useQueryClient();

  // Streaming search state
  const [searchResults, setSearchResults] = useState<SubtitleSearchResult[]>(
    [],
  );
  const [isSearching, setIsSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Abort ongoing search when modal closes
  useEffect(() => {
    if (!open) {
      abortRef.current?.abort();
      abortRef.current = null;
    }
  }, [open]);

  const existingQuery = api.subtitle.getFileSubtitles.useQuery(
    { fileId },
    { enabled: open },
  );

  const downloadMutation = api.subtitle.download.useMutation({
    onSuccess: (sub) => {
      api.subtitle.getFileSubtitles.invalidate(queryClient, { fileId });
      onSubtitleSelected?.(sub);
      setDownloadingId(null);
    },
    onError: () => {
      setDownloadingId(null);
    },
  });

  const deleteMutation = api.subtitle.delete.useMutation({
    onSuccess: (_, subtitleId) => {
      api.subtitle.getFileSubtitles.invalidate(queryClient, { fileId });
      onSubtitleDeleted?.(subtitleId);
      setDeletingId(null);
    },
    onError: () => {
      setDeletingId(null);
    },
  });

  async function handleSearch() {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setSearchResults([]);
    setIsSearching(true);
    setSearchError(null);

    try {
      await api.subtitle.searchStream.stream(
        {
          query: searchQuery,
          imdbId: imdbId ?? undefined,
          tmdbId: tmdbId ?? undefined,
          languages: ["zh-CN", "zh", "zh-TW", "en"],
        },
        (batch) => {
          if (!controller.signal.aborted) {
            setSearchResults((prev) => [...prev, ...batch]);
          }
        },
        controller.signal,
      );
    } catch (e) {
      if (!controller.signal.aborted) {
        setSearchError(e instanceof Error ? e.message : "搜索失败");
      }
    } finally {
      if (!controller.signal.aborted) {
        setIsSearching(false);
      }
    }
  }

  function handleDownload(result: SubtitleSearchResult) {
    setDownloadingId(result.id);
    downloadMutation.mutate({
      fileId,
      subtitleId: result.id,
      detailPath: result.detailPath,
      downloadPath: result.downloadPath,
      language: result.language,
      format: result.format,
      name: result.name,
      provider: result.provider,
    });
  }

  function handleDelete(subtitleId: string) {
    setDeletingId(subtitleId);
    deleteMutation.mutate(subtitleId);
  }

  const groupedExistingSubtitles = groupSubtitles(existingQuery.data ?? []);

  return (
    <Modal
      open={open}
      onCancel={onClose}
      title="字幕管理"
      footer={null}
      width={640}
    >
      <Tabs
        size="small"
        activeKey={activeTab}
        onChange={(key) => setActiveTab(key as TabKey)}
        destroyInactiveTabPane
        className="mb-4"
        items={[
          {
            key: "existing",
            label: "已有字幕",
            children: (
              <div>
                {existingQuery.isLoading ? (
                  <div className="flex justify-center py-8">
                    <Spin />
                  </div>
                ) : !existingQuery.data?.length ? (
                  <p className="py-6 text-center text-sm text-fg-muted">
                    暂无字幕。可切换到「搜索下载」标签从多个字幕源下载。
                  </p>
                ) : (
                  <div className="space-y-4">
                    {groupedExistingSubtitles.map((group) => (
                      <section key={group.key}>
                        <h4 className="mb-2 text-xs font-semibold text-fg-muted">
                          {group.title}
                        </h4>
                        <div className="space-y-2">
                          {group.items.map((sub) => (
                            <div
                              key={sub.id}
                              className="flex items-center justify-between rounded-lg border border-[var(--border-base)] p-3"
                            >
                              <div className="flex items-center gap-2">
                                <span className="text-sm font-medium">
                                  {langLabel(sub.language)}
                                </span>
                                {sub.title && (
                                  <span className="text-xs text-fg-muted">
                                    {sub.title}
                                  </span>
                                )}
                                {sub.isDefault && (
                                  <Tag size="small" color="orange">
                                    默认
                                  </Tag>
                                )}
                                {sub.isForced && (
                                  <Tag size="small" color="red">
                                    强制
                                  </Tag>
                                )}
                                {sub.isHearingImpaired && (
                                  <Tag size="small" color="blue">
                                    听障
                                  </Tag>
                                )}
                              </div>
                              <div className="flex items-center gap-2">
                                <Tag size="small" color="default">
                                  {sub.format.toUpperCase()}
                                </Tag>
                                {sub.storageUrl && onSubtitleSelected && (
                                  <button
                                    type="button"
                                    className="text-xs text-[var(--accent)] hover:underline"
                                    onClick={() => {
                                      onSubtitleSelected(sub);
                                      onClose();
                                    }}
                                  >
                                    使用
                                  </button>
                                )}
                                {sub.sourceType === "downloaded" && (
                                  <button
                                    type="button"
                                    className="text-xs text-red-400 hover:underline disabled:opacity-50"
                                    disabled={deletingId === sub.id}
                                    onClick={() => handleDelete(sub.id)}
                                  >
                                    {deletingId === sub.id ? "删除中" : "删除"}
                                  </button>
                                )}
                              </div>
                            </div>
                          ))}
                        </div>
                      </section>
                    ))}
                  </div>
                )}
              </div>
            ),
          },
          {
            key: "search",
            label: "搜索下载",
            children: (
              <div>
                <div className="mb-3 flex gap-2">
                  <input
                    type="text"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                    placeholder="搜索字幕（建议输入片名或文件名）"
                    className="flex-1 rounded-md border border-[var(--border-base)] bg-[var(--bg-surface)] px-3 py-2 text-sm text-[var(--text-primary)] placeholder:text-fg-muted focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/50"
                  />
                  <button
                    type="button"
                    className="rounded-md bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white hover:opacity-90 disabled:opacity-60"
                    onClick={handleSearch}
                    disabled={isSearching}
                  >
                    {isSearching ? "搜索中" : "搜索"}
                  </button>
                </div>

                {isSearching && (
                  <div className="mb-2 flex items-center gap-2 text-xs text-fg-muted">
                    <Spin size="small" />
                    <span>
                      正在从多个字幕源搜索…已找到 {searchResults.length} 条
                    </span>
                  </div>
                )}

                {searchError ? (
                  <p className="rounded-lg bg-red-500/10 px-4 py-3 text-sm text-red-400">
                    {searchError}
                  </p>
                ) : searchResults.length === 0 && !isSearching ? (
                  <p className="py-6 text-center text-sm text-fg-muted">
                    输入片名后点击「搜索」，结果将从各字幕源实时流式显示。
                  </p>
                ) : (
                  <div className="max-h-96 space-y-2 overflow-y-auto">
                    {searchResults.map((result) => (
                      <div
                        key={`${result.provider}-${result.id}`}
                        className="flex items-center justify-between rounded-lg border border-[var(--border-base)] p-3"
                      >
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium">
                              {langLabel(result.language)}
                            </span>
                            <Tag size="small" color="default">
                              {result.format.toUpperCase()}
                            </Tag>
                            <Tag size="small" color="blue">
                              {result.provider}
                            </Tag>
                            {result.downloadCount != null && (
                              <span className="text-xs text-fg-muted">
                                ↓{result.downloadCount.toLocaleString()}
                              </span>
                            )}
                          </div>
                          <p className="mt-0.5 truncate text-xs text-fg-muted">
                            {result.name}
                          </p>
                          {result.releaseGroup && (
                            <p className="text-xs text-fg-muted">
                              {result.releaseGroup}
                            </p>
                          )}
                        </div>
                        <button
                          type="button"
                          className="ml-3 flex-shrink-0 rounded-md bg-[var(--accent)]/10 px-3 py-1.5 text-xs font-medium text-[var(--accent)] hover:bg-[var(--accent)]/20 disabled:opacity-50"
                          disabled={downloadingId === result.id}
                          onClick={() => handleDownload(result)}
                        >
                          {downloadingId === result.id ? (
                            <span className="flex items-center gap-1">
                              <Spin size="small" /> 下载中
                            </span>
                          ) : (
                            "下载"
                          )}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ),
          },
        ]}
      />
    </Modal>
  );
}
