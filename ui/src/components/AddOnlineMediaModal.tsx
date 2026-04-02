import { useQueryClient } from "@tanstack/react-query";
import {
  Alert,
  Button,
  Form,
  Input,
  LinkOutlined,
  Modal,
  PlusOutlined,
  Select,
  Spin,
  Tag,
} from "@tokiomo/components";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AnalyzeOnlineMediaResponse } from "@/generated/rust-api";
import { api } from "@/generated/rust-api";
import { getMediaFolderOptionLabel } from "@/lib/media-folder";
import { buildProxiedImageUrl } from "@/lib/poster";
import { useMessage } from "@/system";
import {
  type OnlineMediaAnalyzeResult,
  type StartOnlineMediaDownloadInput,
  type StartOnlineMediaDownloadStartedOutput,
  type TokimoApp,
} from "@/types";

interface AddOnlineMediaModalProps {
  open: boolean;
  onClose: () => void;
  onSuccess?: () => void;
  /** Pre-select a target library (e.g. when opened from a library page) */
  defaultLibraryId?: string;
}

const ns = "media.downloads";

function buildFolderOptions(
  folders: TokimoApp[] | undefined,
  analysis: AnalyzeOnlineMediaResponse | null,
): Array<{ label: string; value: string }> {
  if (!folders) return [];

  const preferredType = analysis?.contentType;
  const sorted = [...folders].sort((left, right) => {
    const leftPreferred = preferredType && left.type === preferredType;
    const rightPreferred = preferredType && right.type === preferredType;

    if (leftPreferred && !rightPreferred) return -1;
    if (!leftPreferred && rightPreferred) return 1;
    return left.sortOrder - right.sortOrder;
  });

  return sorted.map((folder) => ({
    value: folder.id,
    label: getMediaFolderOptionLabel(folder),
  }));
}

function _isSourceSiteFolderType(type: TokimoApp["type"]): boolean {
  return type === "music";
}

function getPreferredFolderId(
  folders: TokimoApp[] | undefined,
  analysis: AnalyzeOnlineMediaResponse | null,
): string | undefined {
  if (!folders?.length || !analysis?.isSupported) {
    return undefined;
  }

  if (analysis.contentType) {
    const matchedByType = folders.filter(
      (folder) => folder.type === analysis.contentType,
    );
    if (matchedByType.length === 1) {
      return matchedByType[0]?.id;
    }
  }

  return undefined;
}

export default function AddOnlineMediaModal({
  open,
  onClose,
  onSuccess,
  defaultLibraryId,
}: AddOnlineMediaModalProps) {
  const { t } = useTranslation();
  const [form] = Form.useForm();
  const message = useMessage();
  const qc = useQueryClient();
  const [analysis, setAnalysis] = useState<AnalyzeOnlineMediaResponse | null>(
    null,
  );
  const targetAppId = Form.useWatch<string | undefined>("targetAppId", form);

  const foldersQuery = api.app.list.useQuery({
    enabled: open,
  });

  const analyzeMutation = api.onlineMedia.analyze.useMutation({
    onSuccess: (result) => {
      setAnalysis(result);
      if (!result.isSupported) {
        message.warning(
          t(`${ns}.onlineMedia.unsupported`, {
            defaultValue: "当前链接暂不支持",
          }),
        );
        return;
      }
    },
    onError: (error) => {
      setAnalysis(null);
      message.error(error instanceof Error ? error.message : String(error));
    },
  });

  const startMutation = api.onlineMedia.startDownload.useMutation();

  useEffect(() => {
    if (!open) {
      form.resetFields();
      setAnalysis(null);
    }
  }, [open, form]);

  useEffect(() => {
    if (!open) {
      return;
    }

    // If a default library is pre-set (e.g. from library page), prefer it
    const preferred =
      defaultLibraryId ?? getPreferredFolderId(foldersQuery.data, analysis);
    form.setFieldValue("targetAppId", preferred);
  }, [analysis, foldersQuery.data, form, open, defaultLibraryId]);

  const folderOptions = useMemo(
    () => buildFolderOptions(foldersQuery.data, analysis),
    [foldersQuery.data, analysis],
  );
  const selectedFolder = useMemo(
    () =>
      foldersQuery.data?.find((folder) => folder.id === targetAppId) ?? null,
    [foldersQuery.data, targetAppId],
  );
  const preferredFolderId = useMemo(
    () => getPreferredFolderId(foldersQuery.data, analysis),
    [foldersQuery.data, analysis],
  );
  const needsManualFolderSelection =
    !!analysis?.isSupported && !preferredFolderId && folderOptions.length > 0;
  const urlPlaceholder = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

  const handleAnalyze = async () => {
    const values = await form.validateFields();
    analyzeMutation.mutate({
      url: values.url,
    });
  };

  const handleStart = async () => {
    const values = await form.validateFields();
    if (!analysis?.isSupported || !analysis.provider) {
      message.warning(
        t(`${ns}.onlineMedia.analyzeFirst`, {
          defaultValue: "请先分析链接并确认来源信息",
        }),
      );
      return;
    }
    if (!targetAppId) {
      message.warning(
        t(`${ns}.onlineMedia.selectFolderAfterAnalyze`, {
          defaultValue:
            "请先分析链接，并在自动匹配失败时手动选择目标媒体文件夹",
        }),
      );
      return;
    }

    const isAudioOnly =
      selectedFolder?.type === "music" || analysis.contentType === "music";

    const payload: StartOnlineMediaDownloadInput = {
      url: values.url,
      targetAppId,
      autoOrganize: true,
      confirmDuplicate: false,
      mediaTitle: analysis.title ?? undefined,
      downloadFormat: isAudioOnly ? "audio_only" : "auto",
      analysis: analysis as unknown as OnlineMediaAnalyzeResult,
    };

    const handleStarted = async (
      result: StartOnlineMediaDownloadStartedOutput,
    ) => {
      message.success(
        t(
          result.action === "restarted"
            ? `${ns}.onlineMedia.restarted`
            : `${ns}.onlineMedia.started`,
          {
            defaultValue:
              result.action === "restarted"
                ? "已重新开始下载任务"
                : "已创建下载任务",
          },
        ),
      );
      await Promise.all([api.downloadManage.list.invalidate(qc)]);
      onSuccess?.();
      onClose();
    };

    const submitDownload = async (input: StartOnlineMediaDownloadInput) => {
      const result = await startMutation.mutateAsync(input);
      if (result.action !== "duplicate") {
        await handleStarted(result);
        return;
      }

      Modal.confirm({
        type: "warning",
        title: t(`${ns}.onlineMedia.duplicateTitle`, {
          defaultValue: "发现重复任务",
        }),
        content: result.message,
        okText: t(`${ns}.onlineMedia.redownload`, {
          defaultValue: "重新下载",
        }),
        cancelText: t(`${ns}.cancel`, { defaultValue: "取消" }),
        onOk: async () => {
          try {
            const confirmedResult = await startMutation.mutateAsync({
              ...input,
              confirmDuplicate: true,
              existingRecordId: result.existingRecordId,
            });

            if (confirmedResult.action === "duplicate") {
              message.warning(confirmedResult.message);
              throw new Error(confirmedResult.message);
            }

            await handleStarted(confirmedResult);
          } catch (error) {
            const errorMessage =
              error instanceof Error ? error.message : String(error);
            message.error(errorMessage);
            throw error;
          }
        },
      });
    };

    try {
      await submitDownload(payload);
    } catch (error) {
      message.error(error instanceof Error ? error.message : String(error));
    }
  };

  const renderAnalysis = () => {
    if (analyzeMutation.isPending) {
      return (
        <div className="flex items-center justify-center rounded-lg border border-dashed border-border-base px-4 py-8 text-fg-muted dark:text-slate-400">
          <Spin size="small" />
          <span className="ml-2">
            {t(`${ns}.onlineMedia.analyzing`, { defaultValue: "正在分析链接" })}
          </span>
        </div>
      );
    }

    if (!analysis) return null;

    if (!analysis.isSupported) {
      return (
        <Alert
          type="warning"
          showIcon
          message={t(`${ns}.onlineMedia.unsupported`, {
            defaultValue: "当前链接暂不支持",
          })}
          description={analysis.warnings.join("\n") || undefined}
        />
      );
    }

    const isMusic =
      analysis.contentType === "music" || selectedFolder?.type === "music";

    return (
      <div className="rounded-xl border border-[var(--glass-border)] bg-white/70 p-4 shadow-sm dark:bg-slate-900/40">
        <div className="flex gap-4">
          {analysis.thumbnailUrl ? (
            <img
              src={buildProxiedImageUrl(analysis.thumbnailUrl)}
              alt={analysis.title ?? analysis.sourceSite ?? "thumbnail"}
              className="h-24 w-40 rounded-lg object-cover"
            />
          ) : (
            <div className="flex h-24 w-40 items-center justify-center rounded-lg bg-fill-tertiary text-fg-muted dark:bg-slate-800">
              <PlusOutlined />
            </div>
          )}

          <div className="min-w-0 flex-1 space-y-2">
            <div className="flex flex-wrap gap-2">
              {analysis.sourceSite && <Tag>{analysis.sourceSite}</Tag>}
              {analysis.provider && (
                <Tag color="processing">
                  {analysis.provider.displayName ?? analysis.provider.name}
                </Tag>
              )}
              {analysis.contentType && (
                <Tag color="blue">{analysis.contentType}</Tag>
              )}
              {isMusic && (
                <Tag color="green">
                  {t(`${ns}.onlineMedia.audioOnly`, {
                    defaultValue: "纯音频",
                  })}
                </Tag>
              )}
              {analysis.requiresAuth && (
                <Tag color="warning">
                  {t(`${ns}.onlineMedia.authRequired`, {
                    defaultValue: "需要登录态",
                  })}
                </Tag>
              )}
            </div>

            <div className="text-sm font-semibold text-fg-primary">
              {analysis.title ??
                t(`${ns}.onlineMedia.noTitle`, { defaultValue: "未返回标题" })}
            </div>

            <div className="grid grid-cols-1 gap-2 text-xs text-fg-muted sm:grid-cols-2">
              <span>
                {t(`${ns}.onlineMedia.uploader`, { defaultValue: "上传者" })}:{" "}
                {analysis.uploader ?? "-"}
              </span>
              <span>
                {t(`${ns}.onlineMedia.duration`, { defaultValue: "时长" })}:{" "}
                {analysis.durationSeconds == null
                  ? "-"
                  : `${Math.floor(analysis.durationSeconds / 60)}m ${analysis.durationSeconds % 60}s`}
              </span>
              {analysis.artist && (
                <span>
                  {t(`${ns}.onlineMedia.artist`, { defaultValue: "艺术家" })}:{" "}
                  {analysis.artist}
                </span>
              )}
              {analysis.albumArtist &&
                analysis.albumArtist !== analysis.artist && (
                  <span>
                    {t(`${ns}.onlineMedia.albumArtist`, {
                      defaultValue: "专辑艺术家",
                    })}
                    : {analysis.albumArtist}
                  </span>
                )}
              {analysis.album && (
                <span>
                  {t(`${ns}.onlineMedia.album`, { defaultValue: "专辑" })}:{" "}
                  {analysis.album}
                </span>
              )}
              {analysis.trackTitle && (
                <span>
                  {t(`${ns}.onlineMedia.trackTitle`, { defaultValue: "曲目" })}:{" "}
                  {analysis.trackTitle}
                </span>
              )}
              {analysis.trackNumber != null && (
                <span>
                  {t(`${ns}.onlineMedia.trackNumber`, {
                    defaultValue: "曲目编号",
                  })}
                  :{" "}
                  {analysis.discNumber != null ? `${analysis.discNumber}-` : ""}
                  {analysis.trackNumber}
                </span>
              )}
              {analysis.genre && (
                <span>
                  {t(`${ns}.onlineMedia.genre`, { defaultValue: "流派" })}:{" "}
                  {analysis.genre}
                </span>
              )}
              {analysis.releaseDate && (
                <span>
                  {t(`${ns}.onlineMedia.releaseDate`, {
                    defaultValue: "发行日期",
                  })}
                  : {analysis.releaseDate}
                </span>
              )}
              <span className="truncate sm:col-span-2">
                {t(`${ns}.onlineMedia.normalizedUrl`, {
                  defaultValue: "标准化链接",
                })}
                : {analysis.normalizedUrl ?? "-"}
              </span>
            </div>

            {analysis.description && (
              <p
                className="line-clamp-3 text-xs text-fg-muted"
                title={analysis.description}
              >
                {analysis.description}
              </p>
            )}

            {analysis.warnings.length > 0 && (
              <Alert
                type="info"
                showIcon
                message={analysis.warnings.join("；")}
              />
            )}
          </div>
        </div>
      </div>
    );
  };

  return (
    <Modal
      title={
        <div className="flex items-center gap-2">
          <LinkOutlined />
          <span>
            {t(`${ns}.onlineMedia.title`, { defaultValue: "添加在线媒体" })}
          </span>
        </div>
      }
      open={open}
      onCancel={onClose}
      onOk={handleStart}
      okText={t(`${ns}.onlineMedia.start`, { defaultValue: "开始下载" })}
      cancelText={t(`${ns}.cancel`, { defaultValue: "取消" })}
      confirmLoading={startMutation.isPending}
      okButtonProps={{
        disabled: !analysis?.isSupported || !analysis.provider || !targetAppId,
      }}
    >
      <div className="space-y-4">
        <Form form={form} layout="vertical">
          <Form.Item
            label={t(`${ns}.onlineMedia.url`, { defaultValue: "链接地址" })}
            name="url"
            rules={[
              {
                required: true,
                message: t(`${ns}.onlineMedia.urlRequired`, {
                  defaultValue: "请输入链接地址",
                }),
              },
              {
                type: "url",
                message: t(`${ns}.onlineMedia.urlInvalid`, {
                  defaultValue: "请输入有效的 URL",
                }),
              },
            ]}
          >
            <div className="flex items-center gap-2">
              <Input
                className="flex-1"
                placeholder={urlPlaceholder}
                onChange={() => {
                  setAnalysis(null);
                  // Only clear library selection when there's no fixed default
                  if (!defaultLibraryId) {
                    form.setFieldValue("targetAppId", undefined);
                  }
                }}
              />
              <Button
                icon={<LinkOutlined />}
                loading={analyzeMutation.isPending}
                onClick={() => void handleAnalyze()}
              >
                {t(`${ns}.onlineMedia.analyze`, { defaultValue: "分析" })}
              </Button>
            </div>
          </Form.Item>

          {renderAnalysis()}

          {/* Hidden form value so validation passes when library is pre-set */}
          <Form.Item name="targetAppId" hidden>
            <input />
          </Form.Item>

          {/* Show selector only when NO default library is pre-set */}
          {!defaultLibraryId &&
            analysis?.isSupported &&
            folderOptions.length > 0 && (
              <Form.Item
                label={t(`${ns}.onlineMedia.targetFolder`, {
                  defaultValue: "媒体文件夹",
                })}
                extra={
                  selectedFolder
                    ? needsManualFolderSelection
                      ? t(
                          `${ns}.onlineMedia.targetFolderNeedsManualSelectionDesc`,
                          {
                            defaultValue:
                              "未找到唯一匹配项，请手动选择媒体文件夹。",
                          },
                        )
                      : t(`${ns}.onlineMedia.targetFolderAutoSelected`, {
                          defaultValue:
                            "已自动选中匹配到的媒体文件夹，你也可以手动修改。",
                        })
                    : undefined
                }
                rules={[
                  {
                    required: true,
                    message: t(`${ns}.onlineMedia.targetFolderRequired`, {
                      defaultValue: "请选择目标媒体文件夹",
                    }),
                  },
                ]}
              >
                <Select
                  loading={foldersQuery.isLoading}
                  options={folderOptions}
                  value={targetAppId}
                  onChange={(v) => form.setFieldValue("targetAppId", v)}
                  placeholder={t(`${ns}.onlineMedia.targetFolderPlaceholder`, {
                    defaultValue: "选择下载完成后写入的媒体文件夹",
                  })}
                />
              </Form.Item>
            )}

          {/* When library is pre-set, show a readOnly badge instead */}
          {defaultLibraryId && selectedFolder && (
            <div className="text-xs text-fg-muted">
              目标应用：
              <span className="font-medium text-fg-primary">
                {selectedFolder.name}
              </span>
            </div>
          )}

          {analysis?.isSupported && folderOptions.length === 0 && (
            <Alert
              type="warning"
              showIcon
              message={t(`${ns}.onlineMedia.noTargetFolders`, {
                defaultValue: "当前没有可用的媒体文件夹",
              })}
              description={t(`${ns}.onlineMedia.noTargetFoldersDesc`, {
                defaultValue: "请先创建媒体文件夹，然后再开始下载。",
              })}
            />
          )}
        </Form>
      </div>
    </Modal>
  );
}
