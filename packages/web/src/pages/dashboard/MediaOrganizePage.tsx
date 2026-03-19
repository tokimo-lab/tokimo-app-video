/**
 * 资源整理页面
 * 扫描本地文件 → TMDB 识别 → 选择目标 → 执行整理
 */

import { Button, Card, HistoryOutlined, ScanOutlined } from "@acme/components";
import type { OrganizeItem, WsJobEvent } from "@acme/types";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ManualMatchModal,
  OrganizeItemList,
  OrganizeReportHistory,
  OrganizeReportModal,
  OrganizeToolbar,
} from "../../components/dashboard/media-organize";
import PathSelector from "../../components/dashboard/PathSelector";
import { useAdultMode, useMessage } from "../../hooks";
import { useSseEvent } from "../../hooks/SseContext";
import { useOrganizeSession } from "../../hooks/useOrganizeSession";
import { trpc } from "../../lib/trpc";

/** 递归替换树中指定 ID 的条目 */
function updateItemInTree(
  items: OrganizeItem[],
  updated: OrganizeItem,
): OrganizeItem[] {
  return items.map((item) => {
    if (item.id === updated.id) return updated;
    if (item.children?.length) {
      return { ...item, children: updateItemInTree(item.children, updated) };
    }
    return item;
  });
}

export default function MediaOrganizePage() {
  const { t } = useTranslation();
  const message = useMessage();
  const utils = trpc.useUtils();
  const { session, isActive, isLoading: sessionLoading } = useOrganizeSession();

  // 源路径
  const [sourcePath, setSourcePath] = useState("");
  // 手动搜索弹窗
  const [manualSearchItemId, setManualSearchItemId] = useState<string | null>(
    null,
  );
  // 报告弹窗
  const [reportOpen, setReportOpen] = useState(false);
  // 历史弹窗
  const [historyOpen, setHistoryOpen] = useState(false);
  // 单条识别中的条目 ID
  const [identifyingItemId, setIdentifyingItemId] = useState<string | null>(
    null,
  );
  // 过滤已整理条目
  const [hideOrganized, setHideOrganized] = useState(false);

  // 媒体文件夹列表（为 target 选择器提供选项）
  const foldersQuery = trpc.mediaFolder.listFolders.useQuery();
  const mediaFolders = foldersQuery.data ?? [];

  // 全局成人模式开关
  const { enabled: adultModeEnabled } = useAdultMode();

  // ==================== SSE 实时推送 ====================

  useSseEvent(
    useCallback(
      (event: WsJobEvent) => {
        if (event.type === "organize_item_update") {
          utils.mediaOrganize.getSession.setData(undefined, (old) => {
            if (!old) return old;
            return {
              ...old,
              items: updateItemInTree(old.items, event.item),
              progress: event.progress ?? old.progress,
            };
          });
        } else if (event.type === "organize_status_update") {
          utils.mediaOrganize.getSession.setData(undefined, (old) => {
            if (!old) return old;
            return {
              ...old,
              status: event.status,
              progress: event.progress ?? old.progress,
            };
          });
          // 终态时 invalidate 兜底（确保 report 等数据完整）
          const terminal = new Set(["identified", "done", "scanned"]);
          if (terminal.has(event.status)) {
            utils.mediaOrganize.getSession.invalidate();
          }
        }
      },
      [utils],
    ),
  );

  // ==================== Mutations ====================

  const scanMutation = trpc.mediaOrganize.scan.useMutation({
    onSuccess: () => {
      utils.mediaOrganize.getSession.invalidate();
      message.success(t("media.organize.status.scanned"));
    },
    onError: (err) => message.error(err.message),
  });

  const identifyItemMutation = trpc.mediaOrganize.identifyItem.useMutation({
    onSuccess: (updatedItem) => {
      utils.mediaOrganize.getSession.setData(undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
      setIdentifyingItemId(null);
    },
    onError: (err) => {
      message.error(err.message);
      setIdentifyingItemId(null);
    },
  });

  const identifyAllMutation = trpc.mediaOrganize.identifyAll.useMutation({
    onError: (err) => message.error(err.message),
  });

  const selectMatchMutation = trpc.mediaOrganize.selectMatch.useMutation({
    onSuccess: (updatedItem) => {
      utils.mediaOrganize.getSession.setData(undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const selectAdultMatchMutation =
    trpc.mediaOrganize.selectAdultMatch.useMutation({
      onSuccess: (updatedItem) => {
        utils.mediaOrganize.getSession.setData(undefined, (old) => {
          if (!old) return old;
          return { ...old, items: updateItemInTree(old.items, updatedItem) };
        });
      },
      onError: (err) => message.error(err.message),
    });

  const selectMusicMatchMutation =
    trpc.mediaOrganize.selectMusicMatch.useMutation({
      onSuccess: (updatedItem) => {
        utils.mediaOrganize.getSession.setData(undefined, (old) => {
          if (!old) return old;
          return { ...old, items: updateItemInTree(old.items, updatedItem) };
        });
      },
      onError: (err) => message.error(err.message),
    });

  const resetMatchMutation = trpc.mediaOrganize.resetMatch.useMutation({
    onSuccess: (updatedItem) => {
      utils.mediaOrganize.getSession.setData(undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const updateTargetMutation = trpc.mediaOrganize.updateTarget.useMutation({
    onSuccess: (updatedItem) => {
      // 直接更新缓存以立即反映变更，避免依赖 invalidate + refetch
      utils.mediaOrganize.getSession.setData(undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const executeMutation = trpc.mediaOrganize.execute.useMutation({
    onError: (err) => message.error(err.message),
  });

  const cancelMutation = trpc.mediaOrganize.cancel.useMutation({
    onSuccess: () => utils.mediaOrganize.getSession.invalidate(),
    onError: (err) => message.error(err.message),
  });

  const clearMutation = trpc.mediaOrganize.clear.useMutation({
    onSuccess: () => {
      utils.mediaOrganize.getSession.invalidate();
      setSourcePath("");
    },
    onError: (err) => message.error(err.message),
  });

  // ==================== Handlers ====================

  const handleScan = useCallback(() => {
    const path = sourcePath.trim();
    if (!path) return;
    scanMutation.mutate({ path });
  }, [sourcePath, scanMutation]);

  const handleIdentifyItem = useCallback(
    (itemId: string) => {
      setIdentifyingItemId(itemId);
      identifyItemMutation.mutate({ itemId });
    },
    [identifyItemMutation],
  );

  const handleSelectMatch = useCallback(
    (itemId: string, tmdbId: number, mediaType: "movie" | "tv") => {
      selectMatchMutation.mutate({ itemId, tmdbId, mediaType });
    },
    [selectMatchMutation],
  );

  const handleManualSelect = useCallback(
    (itemId: string, tmdbId: number, mediaType: "movie" | "tv") => {
      selectMatchMutation.mutate({ itemId, tmdbId, mediaType });
    },
    [selectMatchMutation],
  );

  const handleAdultSelect = useCallback(
    (itemId: string, videoId: string) => {
      selectAdultMatchMutation.mutate({ itemId, videoId });
    },
    [selectAdultMatchMutation],
  );

  const handleMusicSelect = useCallback(
    (itemId: string, mbReleaseId: string) => {
      selectMusicMatchMutation.mutate({ itemId, mbReleaseId });
    },
    [selectMusicMatchMutation],
  );

  const handleSelectMusicMatch = useCallback(
    (itemId: string, mbReleaseId: string) => {
      selectMusicMatchMutation.mutate({ itemId, mbReleaseId });
    },
    [selectMusicMatchMutation],
  );

  const handleResetMatch = useCallback((itemId: string) => {
    setManualSearchItemId(itemId);
  }, []);

  const handleCancelMatch = useCallback(
    (itemId: string) => {
      resetMatchMutation.mutate({ itemId });
    },
    [resetMatchMutation],
  );

  /** 查找指定 ID 条目的 contentType（递归搜索树） */
  const findItemContentType = useCallback(
    (itemId: string | null): string | undefined => {
      if (!itemId || !session) return undefined;
      const find = (items: OrganizeItem[]): string | undefined => {
        for (const item of items) {
          if (item.id === itemId) return item.parsed.contentType;
          if (item.children?.length) {
            const found = find(item.children);
            if (found) return found;
          }
        }
        return undefined;
      };
      return find(session.items);
    },
    [session],
  );

  /** 查找指定 ID 条目的文件名（递归搜索树） */
  const findItemFileName = useCallback(
    (itemId: string | null): string | undefined => {
      if (!itemId || !session) return undefined;
      const find = (items: OrganizeItem[]): string | undefined => {
        for (const item of items) {
          if (item.id === itemId) return item.parsed.title || item.fileName;
          if (item.children?.length) {
            const found = find(item.children);
            if (found) return found;
          }
        }
        return undefined;
      };
      return find(session.items);
    },
    [session],
  );

  const handleUpdateTarget = useCallback(
    (itemId: string, folderId?: string, linkMode?: string) => {
      updateTargetMutation.mutate({
        itemId,
        folderId,
        linkMode: linkMode as
          | "hardlink"
          | "softlink"
          | "copy"
          | "move"
          | undefined,
      });
    },
    [updateTargetMutation],
  );

  // Sync sourcePath from session when session changes
  const effectivePath = session?.sourcePath || sourcePath;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
        <div>
          <h1 className="text-xl font-semibold">{t("media.organize.title")}</h1>
          <p className="text-sm text-gray-500 dark:text-gray-400">
            {t("media.organize.subtitle")}
          </p>
        </div>
        <Button icon={<HistoryOutlined />} onClick={() => setHistoryOpen(true)}>
          {t("media.organize.history.title")}
        </Button>
      </div>

      {/* Source path selector */}
      <Card size="small">
        <div className="flex w-full">
          <PathSelector
            value={effectivePath}
            onChange={setSourcePath}
            placeholder={t("media.organize.sourcePathPlaceholder")}
            disabled={isActive}
          />
          <Button
            variant="primary"
            icon={<ScanOutlined />}
            onClick={handleScan}
            loading={scanMutation.isPending}
            disabled={isActive || !sourcePath.trim()}
            className="!rounded-l-none"
          >
            {t("media.organize.scanButton")}
          </Button>
        </div>
      </Card>

      {/* Toolbar — show when session has items */}
      {session && session.items.length > 0 && (
        <Card size="small">
          <OrganizeToolbar
            session={session}
            onIdentifyAll={() => identifyAllMutation.mutate()}
            onExecute={() => executeMutation.mutate({})}
            onCancel={() => cancelMutation.mutate()}
            onClear={() => clearMutation.mutate()}
            onViewReport={() => setReportOpen(true)}
            identifyAllLoading={identifyAllMutation.isPending}
            executeLoading={executeMutation.isPending}
            hideOrganized={hideOrganized}
            onToggleHideOrganized={() => setHideOrganized((v) => !v)}
          />
        </Card>
      )}

      {/* Item list table */}
      {session && session.items.length > 0 && (
        <Card size="small" styles={{ body: { padding: 0 } }}>
          <OrganizeItemList
            items={session.items}
            mediaFolders={mediaFolders}
            loading={sessionLoading}
            sessionStatus={session.status}
            onIdentifyItem={handleIdentifyItem}
            onSelectMatch={handleSelectMatch}
            onSelectMusicMatch={handleSelectMusicMatch}
            onManualSearch={(itemId: string) => setManualSearchItemId(itemId)}
            onResetMatch={handleResetMatch}
            onUpdateTarget={handleUpdateTarget}
            identifyingItemId={identifyingItemId}
            hideOrganized={hideOrganized}
          />
        </Card>
      )}

      {/* Manual search modal */}
      <ManualMatchModal
        open={manualSearchItemId !== null}
        itemId={manualSearchItemId}
        contentType={findItemContentType(manualSearchItemId)}
        adultModeEnabled={adultModeEnabled}
        initialKeyword={findItemFileName(manualSearchItemId)}
        onClose={() => setManualSearchItemId(null)}
        onSelect={handleManualSelect}
        onSelectAdult={handleAdultSelect}
        onSelectMusic={handleMusicSelect}
        onCancelMatch={handleCancelMatch}
      />

      {/* Report modal */}
      <OrganizeReportModal
        open={reportOpen}
        report={session?.report ?? null}
        onClose={() => setReportOpen(false)}
      />

      {/* History modal */}
      <OrganizeReportHistory
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
      />
    </div>
  );
}
