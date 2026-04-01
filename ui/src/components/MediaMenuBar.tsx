import { useQueryClient } from "@tanstack/react-query";
import { Checkbox, Modal } from "@tokiomo/components";
import { FolderSync, Plus, RefreshCw, Scan } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { api } from "@/generated/rust-api";
import type { MenuBarConfig } from "@/system";
import { useLang, useMenuBar, useMessage, useWindowNav } from "@/system";
import AddOnlineMediaModal from "./AddOnlineMediaModal";

export default function MediaMenuBar({ children }: { children: ReactNode }) {
  const { metadata, navigate } = useWindowNav();
  const id = metadata.appId as string | undefined;
  const message = useMessage();
  const qc = useQueryClient();
  const { lang: _lang } = useLang();

  const [syncModalOpen, setSyncModalOpen] = useState(false);
  const [syncClearData, setSyncClearData] = useState(false);
  const [addOnlineModalOpen, setAddOnlineModalOpen] = useState(false);

  const libraryQuery = api.app.getById.useQuery({ id: id! }, { enabled: !!id });
  const library = libraryQuery.data;
  const libType = library?.type ?? "movie";
  const isTv = libType === "tv" || libType === "anime";
  const isOnlineVideo = libType === "online_video";

  const syncMutation = api.app.sync.useMutation({
    onSuccess: () => {
      message.success("同步已开始");
      api.app.listMovies.invalidate(qc);
      api.app.listTvShows.invalidate(qc);
      api.app.getRecentlyAdded.invalidate(qc);
    },
    onError: (e) => message.error(e.message || "同步失败"),
  });

  const scrapePersonsMutation = api.app.scrapeAppPersons.useMutation({
    onSuccess: (data: { queued: number }) =>
      message.success(`已派发 ${data.queued} 个演员 TMDB 刮削任务`),
    onError: (error: Error) => message.error(error.message || "刮削失败"),
  });

  const menuBarConfig: MenuBarConfig | null = useMemo(() => {
    if (!id) return null;

    type MediaItem = { id: string; title?: string | null };

    const actionItems = [
      {
        key: "refresh",
        label: "刷新",
        icon: <RefreshCw size={14} />,
        onClick: () => {
          api.app.listMovies.invalidate(qc);
          api.app.listTvShows.invalidate(qc);
          api.app.getRecentlyAdded.invalidate(qc);
        },
      },
      ...(isOnlineVideo
        ? [
            {
              key: "add-online",
              label: "添加网片",
              icon: <Plus size={14} />,
              onClick: () => setAddOnlineModalOpen(true),
            },
          ]
        : [
            {
              key: "scrape-persons",
              label: "刮削演员",
              icon: <Scan size={14} />,
              disabled: scrapePersonsMutation.isPending,
              onClick: () => scrapePersonsMutation.mutate({ appId: id }),
            },
          ]),
      { type: "divider" as const },
      {
        key: "sync",
        label: "同步资料库",
        icon: <FolderSync size={14} />,
        disabled: syncMutation.isPending,
        onClick: () => {
          setSyncClearData(false);
          setSyncModalOpen(true);
        },
      },
    ];

    return {
      menus: [{ key: "actions", label: "操作", items: actionItems }],
      search: {
        appId: id,
        searchType: isTv ? ("tv" as const) : ("movie" as const),
        onSelect: (item: MediaItem) => {
          if (isTv) {
            navigate(`/tv/${item.id}`, item.title ?? "TV Show");
          } else {
            navigate(`/movies/${item.id}`, item.title ?? "Movie");
          }
        },
      },
    };
  }, [
    id,
    qc,
    isTv,
    isOnlineVideo,
    navigate,
    syncMutation.isPending,
    scrapePersonsMutation.isPending,
    scrapePersonsMutation.mutate,
  ]);

  useMenuBar(menuBarConfig);

  return (
    <>
      {children}

      <Modal
        open={syncModalOpen}
        title="同步应用"
        okText="开始同步"
        cancelText="取消"
        confirmLoading={syncMutation.isPending}
        onCancel={() => setSyncModalOpen(false)}
        onOk={async () => {
          try {
            await syncMutation.mutateAsync({
              id: id!,
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
          勾选后将删除应用中所有已有条目并重新完整同步，适合修复数据异常。
        </p>
      </Modal>

      {isOnlineVideo && (
        <AddOnlineMediaModal
          open={addOnlineModalOpen}
          onClose={() => setAddOnlineModalOpen(false)}
          onSuccess={() => {
            api.downloadManage.list.invalidate(qc);
            message.success("下载任务已创建");
          }}
          defaultLibraryId={id}
        />
      )}
    </>
  );
}
