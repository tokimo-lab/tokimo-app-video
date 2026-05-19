/**
 * 资源整理弹窗
 * 从 VfsConnectionsPage 打开，扫描文件 → TMDB 识别 → 选择目标 → 执行整理
 */

import { useQueryClient } from "@tanstack/react-query";
import { useToast as useMessage } from "@tokimo/sdk";
import { Button, Card, HistoryOutlined, Modal, ScanOutlined } from "@tokimo/ui";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type OrganizeItem } from "../api";
import { useAdultMode } from "../hooks/useAdultMode";
import {
  ManualMatchModal,
  OrganizeItemList,
  OrganizeReportHistory,
  OrganizeReportModal,
  OrganizeToolbar,
  useOrganizeSession,
} from "../shell-shim/apps-media-organize";
import PathSelector from "../shell-shim/apps-settings";

export interface OrganizeDialogProps {
  open: boolean;
  onClose: () => void;
  /** 预填的来源路径 */
  initialSourcePath?: string;
  /** 文件系统 ID */
  fileSystemId?: string;
  /** 来源名称（显示在标题中） */
  sourceName?: string;
}

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

export default function OrganizeDialog({
  open,
  onClose,
  initialSourcePath = "",
  fileSystemId,
  sourceName,
}: OrganizeDialogProps) {
  const { t } = useTranslation();
  const message = useMessage();
  const qc = useQueryClient();
  const { session, isActive, isLoading: sessionLoading } = useOrganizeSession();

  // 源路径
  const [sourcePath, setSourcePath] = useState(initialSourcePath);

  // 弹窗打开时同步初始路径
  useEffect(() => {
    if (open) setSourcePath(initialSourcePath);
  }, [open, initialSourcePath]);
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

  // 应用列表（为 target 选择器提供选项）
  const foldersQuery = api.video.list.useQuery();
  const mediaFolders = foldersQuery.data ?? [];

  // 全局成人模式开关
  const { enabled: adultModeEnabled } = useAdultMode();

  // ==================== Mutations ====================

  const scanMutation = api.mediaOrganize.scan.useMutation({
    onSuccess: () => {
      api.mediaOrganize.getSession.invalidate(qc);
      message.success(t("media.organize.status.scanned"));
    },
    onError: (err) => message.error(err.message),
  });

  const identifyItemMutation = api.mediaOrganize.identifyItem.useMutation({
    onSuccess: (updatedItem) => {
      api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
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

  const identifyAllMutation = api.mediaOrganize.identifyAll.useMutation({
    onError: (err) => message.error(err.message),
  });

  const selectMatchMutation = api.mediaOrganize.selectMatch.useMutation({
    onSuccess: (updatedItem) => {
      api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const selectAdultMatchMutation =
    api.mediaOrganize.selectAdultMatch.useMutation({
      onSuccess: (updatedItem) => {
        api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
          if (!old) return old;
          return { ...old, items: updateItemInTree(old.items, updatedItem) };
        });
      },
      onError: (err) => message.error(err.message),
    });

  const selectMusicMatchMutation =
    api.mediaOrganize.selectMusicMatch.useMutation({
      onSuccess: (updatedItem) => {
        api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
          if (!old) return old;
          return { ...old, items: updateItemInTree(old.items, updatedItem) };
        });
      },
      onError: (err) => message.error(err.message),
    });

  const resetMatchMutation = api.mediaOrganize.resetMatch.useMutation({
    onSuccess: (updatedItem) => {
      api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const updateTargetMutation = api.mediaOrganize.updateTarget.useMutation({
    onSuccess: (updatedItem) => {
      // 直接更新缓存以立即反映变更，避免依赖 invalidate + refetch
      api.mediaOrganize.getSession.setData(qc, undefined, (old) => {
        if (!old) return old;
        return { ...old, items: updateItemInTree(old.items, updatedItem) };
      });
    },
    onError: (err) => message.error(err.message),
  });

  const executeMutation = api.mediaOrganize.execute.useMutation({
    onError: (err) => message.error(err.message),
  });

  const cancelMutation = api.mediaOrganize.cancel.useMutation({
    onSuccess: () => api.mediaOrganize.getSession.invalidate(qc),
    onError: (err) => message.error(err.message),
  });

  const clearMutation = api.mediaOrganize.clear.useMutation({
    onSuccess: () => {
      api.mediaOrganize.getSession.invalidate(qc);
      setSourcePath("");
    },
    onError: (err) => message.error(err.message),
  });

  // ==================== Handlers ====================

  const handleScan = useCallback(() => {
    const path = sourcePath.trim();
    if (!path) return;
    scanMutation.mutate({ path, sourceId: fileSystemId });
  }, [sourcePath, fileSystemId, scanMutation]);

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

  const title = sourceName
    ? `${t("media.organize.title")} — ${sourceName}`
    : t("media.organize.title");

  return (
    <Modal
      open={open}
      onCancel={onClose}
      title={title}
      footer={null}
      width="95%"
      style={{ maxWidth: 1400, top: 24 }}
      styles={{
        body: {
          maxHeight: "calc(100% - 120px)",
          overflowY: "auto",
          padding: "16px 24px",
        },
      }}
      destroyOnClose
    >
      <div className="space-y-4">
        {/* TODO(phase-6): organize feature temporarily disabled */}
        <div className="mx-4 mt-3 rounded-lg bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 px-4 py-2.5 text-sm text-amber-700 dark:text-amber-400">
          {t("media.organize.notAvailable")}
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

        {/* History button */}
        <div className="flex justify-end">
          <Button
            icon={<HistoryOutlined />}
            onClick={() => setHistoryOpen(true)}
          >
            {t("media.organize.history.title")}
          </Button>
        </div>
      </div>

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
    </Modal>
  );
}
