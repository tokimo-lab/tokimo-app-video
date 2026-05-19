import { useQueryClient } from "@tanstack/react-query";
import { Checkbox, Modal } from "@tokimo/ui";
import { FolderSync, Plus, RefreshCw } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { api } from "../shell-shim/api";
import type { MenuBarConfig } from "../shell-shim/system";
import {
  useMenuBar,
  useMessage,
  useWindowActions,
  useWindowId,
} from "../shell-shim/system";
import type { TaskMetadata } from "../shell-shim/system-window-types";
import { useActiveLibrary } from "./ActiveLibraryContext";

export default function VideoMenuBar({ children }: { children: ReactNode }) {
  const message = useMessage();
  const qc = useQueryClient();
  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();
  const activeLibrary = useActiveLibrary();

  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [syncClearData, setSyncClearData] = useState(false);
  const [syncTargetId, setSyncTargetId] = useState<string | null>(null);
  const [syncTargetName, setSyncTargetName] = useState<string>("");

  const categoriesQuery = api.video.list.useQuery();
  const categories = categoriesQuery.data ?? [];

  const syncMutation = api.video.sync.useMutation({
    onSuccess: () => {
      message.success("同步已开始");
      api.video.list.invalidate(qc);
      api.video.listVideoItems.invalidate(qc);
      api.video.listTvShows.invalidate(qc);
      api.video.getRecentlyAdded.invalidate(qc);
      api.video.listGenres.invalidate(qc);
    },
    onError: (e) => message.error(e.message || "同步失败"),
  });

  const isOnlineVideo = activeLibrary.type === "online_video";

  const menuBarConfig: MenuBarConfig | null = useMemo(() => {
    const syncItems = categories.map((cat) => ({
      key: `sync-${cat.id}`,
      label: `同步「${cat.name}」`,
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
        label: "添加网片",
        icon: <Plus size={14} />,
        onClick: () => {
          openModalWindow({
            component: () =>
              import("@/apps/video/components/AddOnlineMediaWindow"),
            parentWindowId: windowId,
            title: "添加在线媒体",
            width: 680,
            height: 680,
            noResize: true,
            noMinimize: true,
            metadata: activeLibrary.id
              ? ({
                  defaultLibraryId: activeLibrary.id,
                } as Record<string, unknown> as TaskMetadata)
              : undefined,
          });
        },
      });
    }

    return {
      menus: [
        {
          key: "actions",
          label: "操作",
          items: [
            {
              key: "refresh",
              label: "刷新",
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
      ],
    };
  }, [
    categories,
    qc,
    isOnlineVideo,
    activeLibrary.id,
    windowId,
    openModalWindow,
  ]);

  useMenuBar(menuBarConfig);

  return (
    <>
      {children}

      <Modal
        open={syncModalOpen}
        title={`同步「${syncTargetName}」`}
        okText="开始同步"
        cancelText="取消"
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
          清空数据重新同步
        </Checkbox>
        <p className="mt-2 text-xs text-[var(--text-muted)]">
          勾选后将删除该分类中所有已有条目并重新完整同步，适合修复数据异常。
        </p>
      </Modal>
    </>
  );
}
