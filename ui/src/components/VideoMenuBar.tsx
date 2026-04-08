import { useQueryClient } from "@tanstack/react-query";
import { Checkbox, Modal } from "@tokiomo/components";
import { FolderSync, RefreshCw } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { api } from "@/generated/rust-api";
import type { MenuBarConfig } from "@/system";
import { useMenuBar, useMessage } from "@/system";

export default function VideoMenuBar({ children }: { children: ReactNode }) {
  const message = useMessage();
  const qc = useQueryClient();

  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [syncClearData, setSyncClearData] = useState(false);
  const [syncTargetId, setSyncTargetId] = useState<string | null>(null);
  const [syncTargetName, setSyncTargetName] = useState<string>("");

  const categoriesQuery = api.video.list.useQuery();
  const categories = categoriesQuery.data ?? [];

  const syncMutation = api.video.sync.useMutation({
    onSuccess: () => {
      message.success("同步已开始");
      api.video.listVideoItems.invalidate(qc);
      api.video.listTvShows.invalidate(qc);
      api.video.getRecentlyAdded.invalidate(qc);
    },
    onError: (e) => message.error(e.message || "同步失败"),
  });

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
            ...(syncItems.length > 0
              ? [{ type: "divider" as const }, ...syncItems]
              : []),
          ],
        },
      ],
    };
  }, [categories, qc]);

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
