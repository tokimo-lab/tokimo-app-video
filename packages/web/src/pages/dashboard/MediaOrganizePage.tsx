/**
 * 资源整理页面
 * 扫描本地文件 → TMDB 识别 → 选择目标 → 执行整理
 */

import { Button, Card, HistoryOutlined, ScanOutlined } from "@acme/components";
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
import { useMessage } from "../../hooks";
import { useOrganizeSession } from "../../hooks/useOrganizeSession";
import { trpc } from "../../lib/trpc";

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

  // 媒体文件夹列表（为 target 选择器提供选项）
  const foldersQuery = trpc.mediaFolder.listFolders.useQuery();
  const mediaFolders = foldersQuery.data ?? [];

  // ==================== Mutations ====================

  const scanMutation = trpc.mediaOrganize.scan.useMutation({
    onSuccess: () => {
      utils.mediaOrganize.getSession.invalidate();
      message.success(t("media.organize.status.scanned"));
    },
    onError: (err) => message.error(err.message),
  });

  const identifyItemMutation = trpc.mediaOrganize.identifyItem.useMutation({
    onSuccess: () => {
      utils.mediaOrganize.getSession.invalidate();
      setIdentifyingItemId(null);
    },
    onError: (err) => {
      message.error(err.message);
      setIdentifyingItemId(null);
    },
  });

  const identifyAllMutation = trpc.mediaOrganize.identifyAll.useMutation({
    onSuccess: () => utils.mediaOrganize.getSession.invalidate(),
    onError: (err) => message.error(err.message),
  });

  const selectMatchMutation = trpc.mediaOrganize.selectMatch.useMutation({
    onSuccess: () => utils.mediaOrganize.getSession.invalidate(),
    onError: (err) => message.error(err.message),
  });

  const updateTargetMutation = trpc.mediaOrganize.updateTarget.useMutation({
    onSuccess: () => utils.mediaOrganize.getSession.invalidate(),
    onError: (err) => message.error(err.message),
  });

  const executeMutation = trpc.mediaOrganize.execute.useMutation({
    onSuccess: () => utils.mediaOrganize.getSession.invalidate(),
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
          <p className="text-sm opacity-60">{t("media.organize.subtitle")}</p>
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
            onManualSearch={(itemId: string) => setManualSearchItemId(itemId)}
            onUpdateTarget={handleUpdateTarget}
            identifyingItemId={identifyingItemId}
          />
        </Card>
      )}

      {/* Manual search modal */}
      <ManualMatchModal
        open={manualSearchItemId !== null}
        itemId={manualSearchItemId}
        onClose={() => setManualSearchItemId(null)}
        onSelect={handleManualSelect}
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
