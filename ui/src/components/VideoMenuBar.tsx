import { useQueryClient } from "@tanstack/react-query";
import {
  type MenuBarConfig,
  useMenuBar,
  useToast as useMessage,
  useRuntimeCtx,
  useWindowActions,
  useWindowId,
} from "@tokimo/sdk";
import { Checkbox, Modal } from "@tokimo/ui";
import { FolderSync, Plus, RefreshCw, Settings } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { registerBridge } from "../modal-bridge";
import { useActiveLibrary } from "./ActiveLibraryContext";

export default function VideoMenuBar({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const message = useMessage();
  const qc = useQueryClient();
  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();
  const activeLibrary = useActiveLibrary();
  const ctx = useRuntimeCtx();

  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [syncClearData, setSyncClearData] = useState(false);
  const [syncTargetId, setSyncTargetId] = useState<string | null>(null);
  const [syncTargetName, setSyncTargetName] = useState<string>("");

  const categoriesQuery = api.video.list.useQuery();
  const categories = categoriesQuery.data ?? [];

  const syncMutation = api.video.sync.useMutation({
    onSuccess: () => {
      message.success(t("media.sidebar.syncStarted"));
      api.video.list.invalidate(qc);
      api.video.listVideoItems.invalidate(qc);
      api.video.listTvShows.invalidate(qc);
      api.video.getRecentlyAdded.invalidate(qc);
      api.video.listGenres.invalidate(qc);
    },
    onError: (e) => message.error(e.message || t("media.sidebar.syncFailed")),
  });

  const isOnlineVideo = activeLibrary.type === "online_video";

  const menuBarConfig: MenuBarConfig | null = useMemo(() => {
    const syncItems = categories.map((cat) => ({
      key: `sync-${cat.id}`,
      label: t("media.sidebar.syncLibrary", { name: cat.name }),
      icon: <FolderSync size={14} />,
      onClick: () => {
        setSyncTargetId(cat.id);
        setSyncTargetName(cat.name);
        setSyncClearData(false);
        setSyncModalOpen(true);
      },
    }));

    const extraItems: Array<{
      key: string;
      label: string;
      icon: ReactNode;
      disabled?: boolean;
      onClick: () => void;
    }> = [];

    if (isOnlineVideo) {
      extraItems.push({
        key: "add-online",
        label: t("media.sidebar.addOnlineMedia"),
        icon: <Plus size={14} />,
        onClick: () => {
          const bridgeId = registerBridge({ kind: "add-online-media", ctx });
          const metadata: Record<string, unknown> = { bridgeId };
          if (activeLibrary.id) metadata.defaultLibraryId = activeLibrary.id;

          openModalWindow({
            component: () => import("./AddOnlineMediaWindow"),
            parentWindowId: windowId,
            title: t("media.downloads.onlineMedia.title"),
            width: 680,
            height: 680,
            noResize: true,
            noMinimize: true,
            metadata,
          });
        },
      });
    }

    return {
      menus: [
        {
          key: "actions",
          label: t("media.sidebar.actions"),
          items: [
            {
              key: "refresh",
              label: t("media.sidebar.refresh"),
              icon: <RefreshCw size={14} />,
              onClick: () => {
                api.video.list.invalidate(qc);
                api.video.listVideoItems.invalidate(qc);
                api.video.listTvShows.invalidate(qc);
                api.video.getRecentlyAdded.invalidate(qc);
              },
            },
            ...(extraItems.length > 0
              ? [{ type: "divider" as const }, ...extraItems]
              : []),
            ...(syncItems.length > 0
              ? [{ type: "divider" as const }, ...syncItems]
              : []),
          ],
        },
        {
          key: "settings",
          label: t("media.menu.settings"),
          items: [
            {
              key: "download-engine-settings",
              label: t("media.downloads.engineSettings.menu"),
              icon: <Settings size={14} />,
              onClick: () => {
                openModalWindow({
                  component: () => import("./DownloadEngineSettingsWindow"),
                  parentWindowId: windowId,
                  title: t("media.downloads.engineSettings.title"),
                  width: 720,
                  height: 640,
                  noMinimize: true,
                });
              },
            },
          ],
        },
      ],
    };
  }, [
    categories,
    qc,
    isOnlineVideo,
    activeLibrary.id,
    windowId,
    openModalWindow,
    ctx,
    t,
  ]);

  useMenuBar(menuBarConfig);

  return (
    <>
      {children}

      <Modal
        open={syncModalOpen}
        title={t("media.sidebar.syncLibrary", { name: syncTargetName })}
        okText={t("media.sidebar.startSync")}
        cancelText={t("media.sidebar.cancel")}
        confirmLoading={syncMutation.isPending}
        onCancel={() => setSyncModalOpen(false)}
        onOk={async () => {
          if (!syncTargetId) return;
          try {
            await syncMutation.mutateAsync({
              id: syncTargetId,
              clearData: syncClearData,
            });
          } finally {
            setSyncModalOpen(false);
          }
        }}
      >
        <Checkbox
          checked={syncClearData}
          onChange={(e) => setSyncClearData(e.target.checked)}
        >
          {t("media.sidebar.clearDataResync")}
        </Checkbox>
        <p className="mt-2 text-xs text-[var(--text-muted)]">
          {t("media.sidebar.clearDataResyncDesc")}
        </p>
      </Modal>
    </>
  );
}
