/**
 * VideoLibraryEditor — inline panel for creating / editing a video library.
 *
 * Embedded in VideoApp's right pane via the inline-settings mode pattern.
 */

import { useQueryClient } from "@tanstack/react-query";
import {
  type AvatarData,
  parseAvatar,
  useToast as useMessage,
  useWindowActions,
  useWindowId,
} from "@tokimo/sdk";
import {
  Button,
  cn,
  Form,
  type FormInstance,
  Input,
  Modal,
  ScrollArea,
} from "@tokimo/ui";
import { Pencil, Settings, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { OrganizeSettings, VideoOutput } from "./api";
import { api } from "./api";
import {
  getDefaultFileFormat,
  getDefaultFolderFormat,
} from "./lib/media-organize";
import { AvatarPicker } from "./shell-shim/components";
import VideoBindingsField, {
  type VideoBinding,
} from "./video-library/VideoBindingsField";
import VideoOrganizeFields from "./video-library/VideoOrganizeFields";
import VideoTypeSelector from "./video-library/VideoTypeSelector";
import { getVideoTypeInfo } from "./video-library/video-types";

interface VideoLibraryEditorProps {
  videoId?: string;
  onSaved?: (savedId: string) => void;
  onDeleted?: () => void;
  onCancel?: () => void;
}

export default function VideoLibraryEditor({
  videoId,
  onSaved,
  onDeleted,
  onCancel,
}: VideoLibraryEditorProps) {
  const { t } = useTranslation();
  const message = useMessage();
  const qc = useQueryClient();
  const windowId = useWindowId();
  const { openModalWindow } = useWindowActions();
  const [form] = Form.useForm();

  const { data: categories = [] } = api.video.list.useQuery();
  const { data: vfsSources = [] } = api.vfs.list.useQuery();
  const video = videoId ? categories.find((c) => c.id === videoId) : undefined;

  const [showTypeSelect, setShowTypeSelect] = useState(!videoId);
  const [selectedType, setSelectedType] = useState<string | undefined>(
    video?.type,
  );
  const [avatar, setAvatar] = useState<AvatarData | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteInput, setDeleteInput] = useState("");

  // Reset state when videoId changes (switching between libraries)
  const prevVideoId = useRef(videoId);
  useEffect(() => {
    if (prevVideoId.current !== videoId) {
      prevVideoId.current = videoId;
      setShowTypeSelect(!videoId);
      setSelectedType(undefined);
      setDeleteOpen(false);
      setDeleteInput("");
    }
  }, [videoId]);

  const initDefaults = useCallback(
    (type: string) => {
      const info = getVideoTypeInfo(type);
      setAvatar({
        type: "icon",
        icon: `lucide:${info.iconName}`,
        color: info.color,
      });
      form.setFieldsValue({
        type,
        bindings: [],
        linkMode: "hardlink",
        folderFormat: getDefaultFolderFormat(type),
        fileFormat: getDefaultFileFormat(type),
        organizeLang: "zh-CN",
        flattenDisc: false,
        fixEmbyDisc: false,
        strictYearMatch: false,
      });
    },
    [form],
  );

  useEffect(() => {
    if (video) {
      const settings = (video.settings ?? {}) as Partial<OrganizeSettings>;
      const vt = video.type as string;
      setSelectedType(vt);
      setAvatar(parseAvatar(video.avatar));
      form.setFieldsValue({
        type: vt,
        name: video.name,
        description: video.description ?? "",
        linkMode: settings.linkMode ?? "hardlink",
        folderFormat: settings.folderFormat || getDefaultFolderFormat(vt),
        fileFormat: settings.fileFormat || getDefaultFileFormat(vt),
        organizeLang: settings.organizeLang || "zh-CN",
        flattenDisc: settings.flattenDisc ?? false,
        fixEmbyDisc: settings.fixEmbyDisc ?? false,
        strictYearMatch: settings.strictYearMatch ?? false,
      });
    } else {
      form.resetFields();
    }
  }, [video, form]);

  // ── Mutations ──
  const createMutation = api.video.create.useMutation();
  const updateMutation = api.video.update.useMutation();

  const deleteMutation = api.video.delete.useMutation({
    onSuccess: () => {
      message.success(t("media.libraryEditor.deleteSuccess"));
      api.video.list.invalidate(qc);
      setDeleteOpen(false);
      onDeleted?.();
    },
    onError: (e) =>
      message.error(e.message || t("media.libraryEditor.deleteFailed")),
  });

  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    const rawBindings =
      (form.getFieldValue("bindings") as VideoBinding[] | undefined) ?? [];
    const sources = rawBindings
      .filter((b) => b.sourceId && b.rootPath)
      .map((b, i) => ({
        sourceId: b.sourceId,
        rootPath: b.rootPath,
        sortOrder: i,
        isDefaultDownload: b.isDefaultDownload ?? i === 0,
      }));

    const settings: Record<string, unknown> = {
      linkMode: values.linkMode ?? "hardlink",
      folderFormat: values.folderFormat?.trim() || null,
      fileFormat: values.fileFormat?.trim() || null,
      organizeLang: values.organizeLang || null,
      flattenDisc: values.flattenDisc ?? false,
      fixEmbyDisc: values.fixEmbyDisc ?? false,
      strictYearMatch: values.strictYearMatch ?? false,
    };

    try {
      let savedId: string;
      if (video) {
        await updateMutation.mutateAsync({
          id: video.id,
          type: selectedType,
          name: values.name as string,
          avatar: avatar as Record<string, unknown> | null,
          description: (values.description as string) || null,
          settings,
          sources,
        });
        savedId = video.id;
        message.success(t("media.libraryEditor.saveSuccess"));
      } else {
        const created = await createMutation.mutateAsync({
          name: values.name as string,
          type: selectedType!,
          avatar: avatar as Record<string, unknown> | null,
          description: (values.description as string) || null,
          settings,
          sources,
        });
        savedId = created.id;
        message.success(t("media.libraryEditor.createSuccess"));
      }
      api.video.list.invalidate(qc);
      onSaved?.(savedId);
    } catch (e) {
      const msg =
        e instanceof Error
          ? e.message
          : video
            ? t("media.libraryEditor.saveFailed")
            : t("media.libraryEditor.createFailed");
      message.error(msg);
    }
  }, [
    form,
    video,
    selectedType,
    avatar,
    createMutation,
    updateMutation,
    qc,
    message,
    onSaved,
    t,
  ]);

  const openDownloadEngineSettings = useCallback(() => {
    openModalWindow({
      component: () => import("./components/DownloadEngineSettingsWindow"),
      parentWindowId: windowId,
      title: t("media.downloads.engineSettings.title"),
      width: 720,
      height: 640,
      noMinimize: true,
    });
  }, [openModalWindow, windowId, t]);

  const isPending = createMutation.isPending || updateMutation.isPending;
  const typeInfo = selectedType ? getVideoTypeInfo(selectedType) : null;

  // ── Type selector step ──
  if (showTypeSelect) {
    return (
      <div className="flex h-full flex-col overflow-hidden">
        <ScrollArea
          direction="vertical"
          className="flex-1"
          innerClassName="px-5 py-5"
        >
          <VideoTypeSelector
            value={selectedType}
            onChange={(t) => setSelectedType(t)}
          />
        </ScrollArea>
        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border-base px-5 py-3">
          <Button variant="default" onClick={onCancel}>
            {t("media.libraryEditor.cancel")}
          </Button>
          <Button
            disabled={!selectedType}
            onClick={() => {
              if (!selectedType) return;
              if (video) {
                form.setFieldsValue({
                  type: selectedType,
                  folderFormat: getDefaultFolderFormat(selectedType),
                  fileFormat: getDefaultFileFormat(selectedType),
                });
              } else {
                initDefaults(selectedType);
              }
              setShowTypeSelect(false);
            }}
          >
            {video
              ? t("media.libraryEditor.confirmSwitch")
              : t("media.libraryEditor.continue")}
          </Button>
        </div>
      </div>
    );
  }

  // ── Main form ──
  return (
    <div className="flex h-full flex-col overflow-hidden">
      {typeInfo && (
        <button
          type="button"
          className={cn(
            "mx-5 mt-4 flex w-[calc(100%-2.5rem)] shrink-0 cursor-pointer items-start gap-3 rounded-xl px-4 py-3 text-left transition-opacity hover:opacity-80",
            typeInfo.bgClass,
          )}
          onClick={() => setShowTypeSelect(true)}
          title={t("media.libraryEditor.switchTypeTooltip")}
        >
          <typeInfo.icon
            className={cn("mt-0.5 h-5 w-5 shrink-0", typeInfo.textClass)}
            aria-hidden
          />
          <div className="flex-1">
            <span className={cn("text-sm font-bold", typeInfo.textClass)}>
              {t(typeInfo.label)}
            </span>
            <p className="mt-0.5 text-xs leading-relaxed text-fg-muted">
              {t(typeInfo.detailedDescription)}
            </p>
          </div>
          <Pencil
            className={cn("mt-0.5 h-3.5 w-3.5 shrink-0", typeInfo.textClass)}
          />
        </button>
      )}

      <Form
        form={form as FormInstance}
        layout="vertical"
        autoComplete="off"
        className="flex min-h-0 flex-1 flex-col"
      >
        <ScrollArea
          direction="vertical"
          className="min-h-0 flex-1"
          innerClassName="space-y-5 px-5 py-5"
        >
          {/* 基本信息 */}
          <div className="rounded-lg border border-border-base p-5">
            <h4 className="mb-4 text-sm font-semibold text-fg-primary">
              {t("media.libraryEditor.basicInfo")}
            </h4>
            <Form.Item name="type" hidden>
              <Input />
            </Form.Item>

            <div className="mb-5">
              <AvatarPicker value={avatar} onChange={setAvatar} size={80} />
            </div>

            <Form.Item
              name="name"
              label={t("media.libraryEditor.name")}
              rules={[
                {
                  required: true,
                  message: t("media.libraryEditor.nameRequired"),
                },
              ]}
            >
              <Input
                placeholder={t("media.libraryEditor.namePlaceholder")}
                size="large"
              />
            </Form.Item>

            <Form.Item
              name="description"
              label={t("media.libraryEditor.description")}
              className="!mb-0"
            >
              <Input.TextArea
                placeholder={t("media.libraryEditor.descriptionPlaceholder")}
                rows={3}
              />
            </Form.Item>
          </div>

          {/* 关联配置 */}
          <div className="rounded-lg border border-border-base p-5">
            <h4 className="mb-4 text-sm font-semibold text-fg-primary">
              {t("media.libraryEditor.bindings")}
            </h4>
            <VideoBindingsField
              sources={vfsSources}
              form={form}
              initialSources={video?.sources}
            />
          </div>

          {selectedType === "online_video" && (
            <div className="rounded-lg border border-border-base p-5">
              <div className="mb-4 flex items-center justify-between">
                <div>
                  <h4 className="text-sm font-semibold text-fg-primary">
                    {t("media.libraryEditor.downloadEngine.title")}
                  </h4>
                  <p className="mt-1 text-xs text-fg-muted">
                    {t("media.libraryEditor.downloadEngine.desc")}
                  </p>
                </div>
                <Button variant="default" onClick={openDownloadEngineSettings}>
                  <Settings size={14} className="mr-1" />
                  {t("media.libraryEditor.downloadEngine.openSettings")}
                </Button>
              </div>
            </div>
          )}

          {/* 整理设置 */}
          <div className="rounded-lg border border-border-base p-5">
            <h4 className="mb-4 text-sm font-semibold text-fg-primary">
              {t("media.libraryEditor.organizeSettings")}
            </h4>
            <VideoOrganizeFields form={form as FormInstance} />
          </div>
        </ScrollArea>

        {/* Footer */}
        <div className="flex shrink-0 items-center justify-between border-t border-border-base px-5 py-3">
          <div>
            {video && (
              <Button variant="danger" onClick={() => setDeleteOpen(true)}>
                <Trash2 size={14} className="mr-1" />
                {t("media.libraryEditor.delete")}
              </Button>
            )}
          </div>
          <div className="flex items-center gap-2">
            {!video && (
              <Button variant="default" onClick={() => setShowTypeSelect(true)}>
                {t("media.libraryEditor.switchType")}
              </Button>
            )}
            <Button variant="default" onClick={onCancel}>
              {t("media.libraryEditor.cancel")}
            </Button>
            <Button loading={isPending} onClick={() => void handleSave()}>
              {video
                ? t("media.libraryEditor.save")
                : t("media.libraryEditor.create")}
            </Button>
          </div>
        </div>
      </Form>

      {/* Delete confirm */}
      {video && (
        <DeleteConfirmModal
          video={video}
          open={deleteOpen}
          deleteInput={deleteInput}
          setDeleteInput={setDeleteInput}
          onCancel={() => {
            setDeleteOpen(false);
            setDeleteInput("");
          }}
          onConfirm={() => deleteMutation.mutate(video.id)}
          loading={deleteMutation.isPending}
          t={t}
        />
      )}
    </div>
  );
}

// ── Delete confirm sub-component ──
function DeleteConfirmModal({
  video,
  open,
  deleteInput,
  setDeleteInput,
  onCancel,
  onConfirm,
  loading,
  t,
}: {
  video: VideoOutput;
  open: boolean;
  deleteInput: string;
  setDeleteInput: (v: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
  loading: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  return (
    <Modal
      title={t("media.libraryEditor.deleteTitle")}
      open={open}
      onCancel={onCancel}
      footer={null}
    >
      <div className="space-y-4 pt-1">
        <p className="text-sm text-fg-secondary">
          {t("media.libraryEditor.deleteConfirmPrefix")}{" "}
          <span className="font-semibold text-fg-primary">{video.name}</span>{" "}
          {t("media.libraryEditor.deleteConfirmMiddle")}
          <span className="font-semibold text-red-500">
            {t("media.libraryEditor.deleteConfirmIrreversible")}
          </span>
          {t("media.libraryEditor.deleteConfirmSuffix")}
        </p>
        <Input
          value={deleteInput}
          onChange={(e) => setDeleteInput(e.target.value)}
          placeholder={video.name}
          onPressEnter={() => {
            if (deleteInput === video.name) onConfirm();
          }}
        />
        <div className="flex justify-end gap-2">
          <Button variant="default" onClick={onCancel}>
            {t("media.libraryEditor.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={deleteInput !== video.name}
            loading={loading}
            onClick={onConfirm}
          >
            {t("media.libraryEditor.confirmDelete")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
