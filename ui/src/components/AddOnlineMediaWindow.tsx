/**
 * Modal window for adding online media (paste URL → analyze → download).
 * Restored from the old AddOnlineMediaModal that was deleted during the apps-table refactor.
 *
 * Receives `win.metadata.defaultLibraryId` to pre-select a target library.
 */
import { useQueryClient } from "@tanstack/react-query";
import {
  buildProxiedImageUrl,
  useToast as useMessage,
  useWindowActions,
  type WindowState,
} from "@tokimo/sdk";
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
import {
  type AnalyzeOnlineMediaResponse,
  api,
  type OnlineMediaAnalyzeResult,
  type StartOnlineMediaDownloadInput,
  type StartOnlineMediaDownloadStartedOutput,
  type VideoOutput,
} from "../api";
import { queryClient } from "../index";
import { getBridge } from "../modal-bridge";
import { withProviders } from "../shared/providers";

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
              <Tag color="success">{t(`${ns}.onlineMedia.audioOnly`)}</Tag>
            )}
            {analysis.requiresAuth && (
              <Tag color="warning">{t(`${ns}.onlineMedia.authRequired`)}</Tag>
            )}
          </div>

          <div className="text-sm font-semibold text-fg-primary">
            {analysis.title ?? t(`${ns}.onlineMedia.noTitle`)}
          </div>

          <div className="grid grid-cols-2 gap-2 text-xs text-fg-muted">
            <span>
              {t(`${ns}.onlineMedia.uploader`)}: {analysis.uploader ?? "-"}
            </span>
            <span>
              {t(`${ns}.onlineMedia.duration`)}:{" "}
              {analysis.durationSeconds == null
                ? "-"
                : `${Math.floor(analysis.durationSeconds / 60)}m ${analysis.durationSeconds % 60}s`}
            </span>
            {analysis.artist && (
              <span>
                {t(`${ns}.onlineMedia.artist`)}: {analysis.artist}
              </span>
            )}
            {analysis.album && (
              <span>
                {t(`${ns}.onlineMedia.album`)}: {analysis.album}
              </span>
            )}
            {analysis.releaseDate && (
              <span>
                {t(`${ns}.onlineMedia.releaseDate`)}: {analysis.releaseDate}
              </span>
            )}
            <span className="col-span-2 truncate">
              {t(`${ns}.onlineMedia.normalizedUrl`)}:{" "}
              {analysis.normalizedUrl ?? "-"}
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

function AddOnlineMediaContent({ win }: { win: WindowState }) {
  const defaultLibraryId =
    typeof win.metadata?.defaultLibraryId === "string"
      ? win.metadata.defaultLibraryId
      : undefined;

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

  const analyzeMutation = api.videoOnlineMedia.analyze.useMutation({
    onSuccess: (result) => {
      setAnalysis(result);
      if (!result.isSupported) {
        message.warning(t(`${ns}.onlineMedia.unsupported`));
      }
    },
    onError: (error) => {
      setAnalysis(null);
      message.error(error instanceof Error ? error.message : String(error));
    },
  });

  const startMutation = api.videoOnlineMedia.startDownload.useMutation();

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
      message.warning(t(`${ns}.onlineMedia.analyzeFirst`));
      return;
    }
    if (!targetLibraryId) {
      message.warning(t(`${ns}.onlineMedia.selectFolderAfterAnalyze`));
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
          undefined,
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
        title: t(`${ns}.onlineMedia.duplicateTitle`),
        content: result.message,
        okText: t(`${ns}.onlineMedia.redownload`),
        cancelText: t(`${ns}.cancel`),
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
            label={t(`${ns}.onlineMedia.url`)}
            name="url"
            rules={[
              {
                required: true,
                message: t(`${ns}.onlineMedia.urlRequired`),
              },
              {
                type: "url",
                message: t(`${ns}.onlineMedia.urlInvalid`),
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
                {t(`${ns}.onlineMedia.analyze`)}
              </Button>
            </div>
          </Form.Item>

          {/* Loading state */}
          {analyzeMutation.isPending && (
            <div className="flex items-center justify-center rounded-lg border border-dashed border-border-base px-4 py-8 text-fg-muted">
              <Spin size="small" />
              <span className="ml-2">{t(`${ns}.onlineMedia.analyzing`)}</span>
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
                message={t(`${ns}.onlineMedia.unsupported`)}
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
                label={t(`${ns}.onlineMedia.targetFolder`)}
                extra={
                  selectedLibrary
                    ? needsManualSelection
                      ? t(
                          `${ns}.onlineMedia.targetFolderNeedsManualSelectionDesc`,
                        )
                      : t(`${ns}.onlineMedia.targetFolderAutoSelected`)
                    : undefined
                }
                rules={[
                  {
                    required: true,
                    message: t(`${ns}.onlineMedia.targetFolderRequired`),
                  },
                ]}
              >
                <Select
                  loading={librariesQuery.isLoading}
                  options={folderOptions}
                  value={targetLibraryId}
                  onChange={(v) => form.setFieldValue("targetLibraryId", v)}
                  placeholder={t(`${ns}.onlineMedia.targetFolderPlaceholder`)}
                />
              </Form.Item>
            )}

          {/* Read-only badge when library is pre-set */}
          {defaultLibraryId && selectedLibrary && (
            <div className="text-xs text-fg-muted">
              {t(`${ns}.onlineMedia.targetFolder`)}:
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
              message={t(`${ns}.onlineMedia.noTargetFolders`)}
              description={t(`${ns}.onlineMedia.noTargetFoldersDesc`)}
            />
          )}
        </Form>
      </div>

      {/* Footer */}
      <div className="flex shrink-0 items-center justify-end gap-3 border-t border-[var(--border-base)] px-6 py-4">
        <Button onClick={() => closeWindow(win.id)}>{t(`${ns}.cancel`)}</Button>
        <Button
          variant="primary"
          disabled={!canStart}
          loading={startMutation.isPending}
          onClick={() => void handleStart()}
        >
          {t(`${ns}.onlineMedia.start`)}
        </Button>
      </div>
    </div>
  );
}

export default function AddOnlineMediaWindow({ win }: { win: WindowState }) {
  const bridgeId =
    typeof win.metadata?.bridgeId === "string"
      ? win.metadata.bridgeId
      : undefined;
  const [bridge] = useState(() => (bridgeId ? getBridge(bridgeId) : undefined));

  if (bridge?.kind !== "add-online-media") return null;

  return withProviders(
    bridge.ctx,
    queryClient,
    <AddOnlineMediaContent win={win} />,
  );
}
