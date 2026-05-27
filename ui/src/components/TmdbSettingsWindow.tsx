import { QueryClientProvider, useQueryClient } from "@tanstack/react-query";
import type { ShellWindowHandle } from "@tokimo/sdk";
import {
  Alert,
  Button,
  ConfigProvider,
  Input,
  InputNumber,
  Progress,
  SettingGroup,
  SettingRow,
  Spin,
  StickySaveBar,
  Tag,
  ToastProvider,
  useToast as useMessage,
} from "@tokimo/ui";
import {
  CheckCircle,
  Eye,
  EyeOff,
  HelpCircle,
  Link2,
  XCircle,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { I18nextProvider, useTranslation } from "react-i18next";
import { api } from "../api";
import i18n from "../i18n";
import { queryClient } from "../index";

function TmdbStatusCard({
  configured,
  loading,
  quota,
  onTest,
  testing,
  testResult,
}: {
  configured: boolean;
  loading: boolean;
  quota?: ReactNode;
  onTest: () => void;
  testing: boolean;
  testResult?: ReactNode;
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-xl border border-border-base bg-surface-base/60 p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            {configured ? (
              <CheckCircle size={16} className="text-success" />
            ) : (
              <XCircle size={16} className="text-error" />
            )}
            <span className="font-medium text-fg-primary">
              {t("media.tmdbSettings.connectionStatus")}
            </span>
            <Tag color={loading ? "default" : configured ? "success" : "error"}>
              {loading
                ? t("media.common.loading")
                : configured
                  ? t("media.tmdbSettings.connected")
                  : t("media.common.notConfigured")}
            </Tag>
          </div>
          {quota}
          {testResult && <div className="text-sm">{testResult}</div>}
        </div>
        <Button loading={testing} onClick={onTest}>
          {t("media.tmdbSettings.testConnection")}
        </Button>
      </div>
    </div>
  );
}

function TmdbSettingsContent() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showApiKey, setShowApiKey] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [dailyLimit, setDailyLimit] = useState<number | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const message = useMessage();

  const settingsQuery = api.externalDb.getTmdb.useQuery();
  const updateSettingsMutation = api.externalDb.updateTmdb.useMutation();
  const testMutation = api.externalDb.testTmdb.useMutation();
  const statusQuery = api.externalDb.status.useQuery();

  const tmdbStatus = statusQuery.data?.tmdb;

  useEffect(() => {
    if (settingsQuery.data) {
      setApiKey(settingsQuery.data.apiKey ?? "");
      setDailyLimit(settingsQuery.data.dailyLimit ?? null);
      setIsDirty(false);
    }
  }, [settingsQuery.data]);

  const markDirty = () => {
    setIsDirty(true);
    testMutation.reset();
  };

  const doSave = async () => {
    await updateSettingsMutation.mutateAsync({
      apiKey: apiKey || null,
      dailyLimit: dailyLimit ?? null,
    });
    setIsDirty(false);
  };

  const handleSave = async () => {
    try {
      await doSave();
      message.success(t("media.tmdbSettings.saveSuccess"));
      await api.externalDb.status.invalidate(queryClient);
    } catch (error) {
      console.error("Failed to save TMDB settings", error);
      message.error(t("media.tmdbSettings.saveFailed"));
    }
  };

  const handleReset = () => {
    if (settingsQuery.data) {
      setApiKey(settingsQuery.data.apiKey ?? "");
      setDailyLimit(settingsQuery.data.dailyLimit ?? null);
      setIsDirty(false);
    }
  };

  const handleTest = async () => {
    if (isDirty) {
      try {
        await doSave();
      } catch (error) {
        console.error("Failed to save TMDB settings before test", error);
        message.error(t("media.tmdbSettings.saveFailed"));
        return;
      }
    }
    testMutation.reset();
    try {
      await testMutation.mutateAsync();
      await api.externalDb.status.invalidate(queryClient);
    } catch (error) {
      console.error("Failed to test TMDB connection", error);
    }
  };

  const usageRatio =
    tmdbStatus?.dailyLimit != null && tmdbStatus.dailyLimit > 0
      ? (tmdbStatus.dailyUsage ?? 0) / tmdbStatus.dailyLimit
      : 0;

  return (
    <div className="flex h-full flex-col">
      <div className="flex-1 overflow-y-auto px-6 py-4 space-y-6">
        <TmdbStatusCard
          configured={
            !statusQuery.isLoading && Boolean(tmdbStatus?.isConfigured)
          }
          loading={statusQuery.isLoading}
          quota={
            !statusQuery.isLoading && (
              <div className="flex flex-col gap-1 text-sm text-fg-muted">
                <span>
                  {t("systemSettings.apiUsage.todayUsed")}:{" "}
                  <strong className="text-fg-primary">
                    {tmdbStatus?.dailyUsage ?? 0}
                  </strong>
                  {tmdbStatus?.dailyLimit != null ? (
                    <>
                      {" / "}
                      {tmdbStatus.dailyLimit}
                      <span className="ml-1">
                        ({t("systemSettings.apiUsage.remaining")}:{" "}
                        {Math.max(
                          0,
                          tmdbStatus.dailyLimit - (tmdbStatus.dailyUsage ?? 0),
                        )}
                        )
                      </span>
                    </>
                  ) : (
                    <span className="ml-1">
                      ({t("systemSettings.apiUsage.unlimited")})
                    </span>
                  )}
                </span>
                {tmdbStatus?.dailyLimit != null &&
                  tmdbStatus.dailyLimit > 0 && (
                    <Progress
                      percent={Math.min(100, Math.round(usageRatio * 100))}
                      size="small"
                      status={usageRatio >= 0.9 ? "exception" : "active"}
                    />
                  )}
              </div>
            )
          }
          onTest={() => void handleTest()}
          testing={testMutation.isPending}
          testResult={
            testMutation.data?.success ? (
              <span className="text-success">
                {t("media.tmdbSettings.testSuccess")}
              </span>
            ) : testMutation.data || testMutation.isError ? (
              <span className="text-error">
                {testMutation.data?.errorMessage ||
                  t("media.tmdbSettings.testFailed")}
              </span>
            ) : undefined
          }
        />

        <SettingGroup title={t("media.tmdbSettings.apiKeyConfig")}>
          {settingsQuery.isLoading ? (
            <div className="py-6">
              <Spin />
            </div>
          ) : (
            <>
              <SettingRow
                orientation="vertical"
                label="TMDB API Key"
                desc={t("media.tmdbSettings.apiKeyExtra")}
              >
                <Input.Password
                  value={apiKey}
                  onChange={(e) => {
                    setApiKey(e.target.value);
                    markDirty();
                  }}
                  placeholder={t("media.tmdbSettings.apiKeyPlaceholder")}
                  visibilityToggle={{
                    visible: showApiKey,
                    onVisibleChange: setShowApiKey,
                  }}
                  iconRender={(visible) =>
                    visible ? <Eye size={14} /> : <EyeOff size={14} />
                  }
                  className="max-w-xl"
                />
              </SettingRow>
              <SettingRow
                orientation="vertical"
                label={t("systemSettings.apiUsage.dailyLimit")}
                desc={t("systemSettings.apiUsage.dailyLimitExtra")}
              >
                <InputNumber
                  value={dailyLimit ?? undefined}
                  onChange={(value) => {
                    setDailyLimit(typeof value === "number" ? value : null);
                    markDirty();
                  }}
                  min={0}
                  placeholder={t("systemSettings.apiUsage.unlimited")}
                  className="max-w-xl w-full"
                />
              </SettingRow>
            </>
          )}
        </SettingGroup>

        <SettingGroup title={t("media.tmdbSettings.howToGetApiKey")}>
          <div className="pt-3">
            <Alert
              type="info"
              showIcon
              icon={<HelpCircle size={14} />}
              message={t("media.tmdbSettings.getApiKeySteps")}
              description={
                <ol className="list-decimal list-inside space-y-2 mt-2">
                  <li>
                    {t("media.tmdbSettings.step1Visit")}{" "}
                    <a
                      className="text-[var(--accent-text)] hover:text-[var(--accent)]"
                      href="https://www.themoviedb.org/signup"
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      {t("media.tmdbSettings.step1Link")}
                    </a>{" "}
                    {t("media.tmdbSettings.step1Register")}
                  </li>
                  <li>
                    {t("media.tmdbSettings.step2Login")}{" "}
                    <a
                      className="text-[var(--accent-text)] hover:text-[var(--accent)]"
                      href="https://www.themoviedb.org/settings/api"
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      {t("media.tmdbSettings.step2Link")}
                    </a>
                  </li>
                  <li>{t("media.tmdbSettings.step3")}</li>
                  <li>{t("media.tmdbSettings.step4")}</li>
                </ol>
              }
            />
          </div>
        </SettingGroup>

        <SettingGroup title={t("media.tmdbSettings.featureDescription")}>
          <div className="pt-3 space-y-3">
            <div>
              <p className="font-medium text-sm text-fg-primary !mb-0.5">
                {t("media.tmdbSettings.mediaSearch")}
              </p>
              <p className="text-fg-muted text-sm !mb-0">
                {t("media.tmdbSettings.mediaSearchDesc")}
              </p>
            </div>
            <div>
              <p className="font-medium text-sm text-fg-primary !mb-0.5">
                {t("media.tmdbSettings.metadataFetch")}
              </p>
              <p className="text-fg-muted text-sm !mb-0">
                {t("media.tmdbSettings.metadataFetchDesc")}
              </p>
            </div>
            <div>
              <p className="font-medium text-sm text-fg-primary !mb-0.5">
                {t("media.tmdbSettings.ptSiteIntegration")}
              </p>
              <p className="text-fg-muted text-sm !mb-0">
                {t("media.tmdbSettings.ptSiteIntegrationDesc")}
              </p>
            </div>
          </div>
        </SettingGroup>

        <SettingGroup title={t("media.tmdbSettings.relatedLinks")}>
          <div className="pt-3 space-y-1.5 text-sm">
            {[
              [
                "https://www.themoviedb.org/",
                t("media.tmdbSettings.tmdbWebsite"),
              ],
              [
                "https://www.themoviedb.org/signup",
                t("media.tmdbSettings.registerAccount"),
              ],
              [
                "https://www.themoviedb.org/settings/api",
                t("media.tmdbSettings.getApiKeyLink"),
              ],
              [
                "https://developer.themoviedb.org/docs",
                t("media.tmdbSettings.apiDocs"),
              ],
            ].map(([href, label]) => (
              <div key={href} className="flex items-center gap-1.5">
                <Link2 size={13} className="text-fg-muted" />
                <a
                  className="text-[var(--accent-text)] hover:text-[var(--accent)]"
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {label}
                </a>
              </div>
            ))}
          </div>
        </SettingGroup>
      </div>

      <StickySaveBar
        dirty={isDirty}
        loading={updateSettingsMutation.isPending}
        onSave={() => void handleSave()}
        onReset={handleReset}
        saveLabel={t("media.common.save")}
        resetLabel={t("media.common.cancel")}
      />
    </div>
  );
}

export function TmdbSettingsSection() {
  return (
    <I18nextProvider i18n={i18n}>
      <ConfigProvider>
        <ToastProvider>
          <QueryClientProvider client={queryClient}>
            <TmdbSettingsContent />
          </QueryClientProvider>
        </ToastProvider>
      </ConfigProvider>
    </I18nextProvider>
  );
}

export default function TmdbSettingsWindow({
  win,
}: {
  win: ShellWindowHandle;
}) {
  void win;
  return <TmdbSettingsSection />;
}
