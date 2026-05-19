/**
 * Modal window for adding online media (paste URL → analyze → download).
 * Restored from the old AddOnlineMediaModal that was deleted during the apps-table refactor.
 *
 * Receives `win.metadata.defaultLibraryId` to pre-select a target library.
 */
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
} from "@tokimo/ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  AnalyzeOnlineMediaResponse,
  VideoOutput,
} from "@/generated/rust-api";
import { api } from "@/generated/rust-api";
import { buildProxiedImageUrl } from "@/lib/poster";
import { useMessage, useWindowActions } from "@/system";
import type { WindowState } from "@/system/window/window-types";
import type {
  OnlineMediaAnalyzeResult,
  StartOnlineMediaDownloadInput,
  StartOnlineMediaDownloadStartedOutput,
} from "@/types";

const ns = "media.downloads";

function buildFolderOptions(
  libraries: VideoOutput[] | undefined,
  analysis: AnalyzeOnlineMediaResponse | null,
): Array<{ label: string; value: string }> {
  if (!libraries) return [];

  const preferredType = analysis?.contentType;
  const sorted = [...libraries].sort((left, right) => {
    const leftPreferred = preferredType && left.type === preferredType;
    const rightPreferred = preferredType && right.type === preferredType;
    if (leftPreferred && !rightPreferred) return -1;
    if (!leftPreferred && rightPreferred) return 1;
    return left.sortOrder - right.sortOrder;
  });

  return sorted.map((lib) => ({
    value: lib.id,
    label: lib.name,
  }));
}

function getPreferredLibraryId(
  libraries: VideoOutput[] | undefined,
  analysis: AnalyzeOnlineMediaResponse | null,
): string | undefined {
  if (!libraries?.length || !analysis?.isSupported) return undefined;
  if (analysis.contentType) {
    const matched = libraries.filter((l) => l.type === analysis.contentType);
    if (matched.length === 1) return matched[0]?.id;
  }
  return undefined;
}

// ── Analysis result card ────────────────────────────────────────────────────

function AnalysisCard({
  analysis,
  selectedLibrary,
}: {
  analysis: AnalyzeOnlineMediaResponse;
  selectedLibrary: VideoOutput | null;
}) {
  const { t } = useTranslation();
  const isMusic =
    analysis.contentType === "music" || selectedLibrary?.type === "music";

  return (
    <div className="rounded-xl border border-[var(--glass-border)] bg-surface-glass p-4 shadow-sm">
      <div className="flex flex-col gap-4 sm:flex-row">
        {analysis.thumbnailUrl ? (
          <img
            src={buildProxiedImageUrl(analysis.thumbnailUrl)}
            alt={analysis.title ?? analysis.sourceSite ?? "thumbnail"}
            className="h-40 w-full shrink-0 rounded-lg object-cover sm:h-24 sm:w-40"
          />
        ) : (
          <div className="flex h-40 w-full shrink-0 items-center justify-center rounded-lg bg-fill-tertiary text-fg-muted dark:bg-white/[0.10] sm:h-24 sm:w-40">
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
              <Tag color="success">
                {t(`${ns}.onlineMedia.audioOnly`, { defaultValue: "纯音频" })}
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

          <div className="grid grid-cols-2 gap-2 text-xs text-fg-muted">
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
            {analysis.album && (
              <span>
                {t(`${ns}.onlineMedia.album`, { defaultValue: "专辑" })}:{" "}
                {analysis.album}
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
            <span className="col-span-2 truncate">
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
}

// ── Main modal window ───────────────────────────────────────────────────────

export default function AddOnlineMediaWindow({ win }: { win: WindowState }) {
  const meta = win.metadata as Record<string, unknown>;
  const defaultLibraryId = (meta?.defaultLibraryId as string) ?? undefined;

  const { t } = useTranslation();
  const [form] = Form.useForm();
  const message = useMessage();
  const qc = useQueryClient();
  const { closeWindow } = useWindowActions();

  const [analysis, setAnalysis] = useState<AnalyzeOnlineMediaResponse | null>(
    null,
  );
  const targetLibraryId = Form.useWatch<string | undefined>(
    "targetLibraryId",
    form,
  );

  const librariesQuery = api.video.list.useQuery();
  const libraries = librariesQuery.data;

  const analyzeMutation = api.onlineMedia.analyze.useMutation({
    onSuccess: (result) => {
      setAnalysis(result);
      if (!result.isSupported) {
        message.warning(
          t(`${ns}.onlineMedia.unsupported`, {
            defaultValue: "当前链接暂不支持",
          }),
        );
      }
    },
    onError: (error) => {
      setAnalysis(null);
      message.error(error instanceof Error ? error.message : String(error));
    },
  });

  const startMutation = api.onlineMedia.startDownload.useMutation();

  // Auto-select library after analysis
  useEffect(() => {
    const preferred =
      defaultLibraryId ?? getPreferredLibraryId(libraries, analysis);
    if (preferred) {
      form.setFieldValue("targetLibraryId", preferred);
    }
  }, [analysis, libraries, form, defaultLibraryId]);

  const folderOptions = useMemo(
    () => buildFolderOptions(libraries, analysis),
    [libraries, analysis],
  );

  const selectedLibrary = useMemo(
    () => libraries?.find((l) => l.id === targetLibraryId) ?? null,
    [libraries, targetLibraryId],
  );

  const preferredId = useMemo(
    () => getPreferredLibraryId(libraries, analysis),
    [libraries, analysis],
  );

  const needsManualSelection =
    !!analysis?.isSupported && !preferredId && folderOptions.length > 0;

  const handleAnalyze = async () => {
    const values = await form.validateFields();
    analyzeMutation.mutate({ url: values.url });
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
    if (!targetLibraryId) {
      message.warning(
        t(`${ns}.onlineMedia.selectFolderAfterAnalyze`, {
          defaultValue: "请先分析链接，并在自动匹配失败时手动选择目标视频库",
        }),
      );
      return;
    }

    const isAudioOnly =
      selectedLibrary?.type === "music" || analysis.contentType === "music";

    const payload: StartOnlineMediaDownloadInput = {
      url: values.url,
      targetAppId: targetLibraryId,
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
      await api.downloadManage.list.invalidate(qc);
      closeWindow(win.id);
    };

    const submitDownload = async (input: StartOnlineMediaDownloadInput) => {
      const result = await startMutation.mutateAsync(input);
      if (result.action !== "duplicate") {
        await handleStarted(result);
        return;
      }

      Modal.confirm({
        variant: "warning",
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
            const confirmed = await startMutation.mutateAsync({
              ...input,
              confirmDuplicate: true,
              existingRecordId: result.existingRecordId,
            });
            if (confirmed.action === "duplicate") {
              message.warning(confirmed.message);
              throw new Error(confirmed.message);
            }
            await handleStarted(confirmed);
          } catch (error) {
            message.error(
              error instanceof Error ? error.message : String(error),
            );
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

  const canStart =
    !!analysis?.isSupported && !!analysis.provider && !!targetLibraryId;

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
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
                placeholder="https://www.youtube.com/watch?v=..."
                onChange={() => {
                  setAnalysis(null);
                  if (!defaultLibraryId) {
                    form.setFieldValue("targetLibraryId", undefined);
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

          {/* Loading state */}
          {analyzeMutation.isPending && (
            <div className="flex items-center justify-center rounded-lg border border-dashed border-border-base px-4 py-8 text-fg-muted">
              <Spin size="small" />
              <span className="ml-2">
                {t(`${ns}.onlineMedia.analyzing`, {
                  defaultValue: "正在分析链接",
                })}
              </span>
            </div>
          )}

          {/* Analysis result */}
          {analysis &&
            !analyzeMutation.isPending &&
            (analysis.isSupported ? (
              <AnalysisCard
                analysis={analysis}
                selectedLibrary={selectedLibrary}
              />
            ) : (
              <Alert
                type="warning"
                showIcon
                message={t(`${ns}.onlineMedia.unsupported`, {
                  defaultValue: "当前链接暂不支持",
                })}
                description={analysis.warnings.join("\n") || undefined}
              />
            ))}

          {/* Hidden field for validation */}
          <Form.Item name="targetLibraryId" hidden>
            <input />
          </Form.Item>

          {/* Library selector (when no default is pre-set) */}
          {!defaultLibraryId &&
            analysis?.isSupported &&
            folderOptions.length > 0 && (
              <Form.Item
                label={t(`${ns}.onlineMedia.targetFolder`, {
                  defaultValue: "目标视频库",
                })}
                extra={
                  selectedLibrary
                    ? needsManualSelection
                      ? t(
                          `${ns}.onlineMedia.targetFolderNeedsManualSelectionDesc`,
                          {
                            defaultValue:
                              "未找到唯一匹配项，请手动选择目标视频库。",
                          },
                        )
                      : t(`${ns}.onlineMedia.targetFolderAutoSelected`, {
                          defaultValue:
                            "已自动选中匹配到的视频库，你也可以手动修改。",
                        })
                    : undefined
                }
                rules={[
                  {
                    required: true,
                    message: t(`${ns}.onlineMedia.targetFolderRequired`, {
                      defaultValue: "请选择目标视频库",
                    }),
                  },
                ]}
              >
                <Select
                  loading={librariesQuery.isLoading}
                  options={folderOptions}
                  value={targetLibraryId}
                  onChange={(v) => form.setFieldValue("targetLibraryId", v)}
                  placeholder={t(`${ns}.onlineMedia.targetFolderPlaceholder`, {
                    defaultValue: "选择下载完成后写入的视频库",
                  })}
                />
              </Form.Item>
            )}

          {/* Read-only badge when library is pre-set */}
          {defaultLibraryId && selectedLibrary && (
            <div className="text-xs text-fg-muted">
              目标视频库：
              <span className="font-medium text-fg-primary">
                {selectedLibrary.name}
              </span>
            </div>
          )}

          {/* No libraries warning */}
          {analysis?.isSupported && folderOptions.length === 0 && (
            <Alert
              type="warning"
              showIcon
              message={t(`${ns}.onlineMedia.noTargetFolders`, {
                defaultValue: "当前没有可用的视频库",
              })}
              description={t(`${ns}.onlineMedia.noTargetFoldersDesc`, {
                defaultValue: "请先创建视频库，然后再开始下载。",
              })}
            />
          )}
        </Form>
      </div>

      {/* Footer */}
      <div className="flex shrink-0 items-center justify-end gap-3 border-t border-[var(--border-base)] px-6 py-4">
        <Button onClick={() => closeWindow(win.id)}>
          {t(`${ns}.cancel`, { defaultValue: "取消" })}
        </Button>
        <Button
          variant="primary"
          disabled={!canStart}
          loading={startMutation.isPending}
          onClick={() => void handleStart()}
        >
          {t(`${ns}.onlineMedia.start`, { defaultValue: "开始下载" })}
        </Button>
      </div>
    </div>
  );
}
