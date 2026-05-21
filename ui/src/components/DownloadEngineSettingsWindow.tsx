import { QueryClientProvider } from "@tanstack/react-query";
import type { ShellWindowHandle } from "@tokimo/sdk";
import {
  Alert,
  Button,
  ConfigProvider,
  Input,
  SettingGroup,
  SettingRow,
  Spin,
  StickySaveBar,
  Tag,
  ToastProvider,
  useToast as useMessage,
} from "@tokimo/ui";
import { CheckCircle, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { I18nextProvider, useTranslation } from "react-i18next";
import { api } from "../api";
import i18n from "../i18n";
import { queryClient } from "../index";

const ns = "media.downloads.engineSettings";

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

interface LocalCookie {
  providerId: string;
  cookie: string;
}

function DownloadEngineSettingsContent() {
  const { t } = useTranslation();
  const message = useMessage();

  // ── yt-dlp status ────────────────────────────────────────────────────────
  const ytdlpStatusQuery = api.video.ytdlpStatus.useQuery();
  const ytdlpStatus = ytdlpStatusQuery.data;
  const [checkedLatestVersion, setCheckedLatestVersion] = useState<
    string | null
  >(null);
  const [isCheckingLatest, setIsCheckingLatest] = useState(false);
  const displayLatestVersion =
    ytdlpStatus?.latestVersion ?? checkedLatestVersion ?? null;
  const isYtdlpAlreadyLatest = Boolean(
    ytdlpStatus?.version &&
      displayLatestVersion &&
      ytdlpStatus.version === displayLatestVersion,
  );
  const updateYtdlpMutation = api.video.updateYtdlp.useMutation({
    onSuccess: (data) => {
      message.success(
        t(`${ns}.ytdlp.updateSuccess`, { version: data.version }),
      );
    },
    onError: (error) => {
      message.error(
        t(`${ns}.ytdlp.updateFailed`, { error: getErrorMessage(error) }),
      );
    },
  });

  const handleCheckLatestVersion = useCallback(async () => {
    setIsCheckingLatest(true);
    try {
      const result = await ytdlpStatusQuery.refetch();
      if (result.error) {
        message.error(
          t(`${ns}.ytdlp.checkLatestFailed`, {
            error: getErrorMessage(result.error),
          }),
        );
        return;
      }

      const latestVersion = result.data?.latestVersion;
      if (!latestVersion) {
        message.error(t(`${ns}.ytdlp.checkLatestNoVersion`));
        return;
      }

      setCheckedLatestVersion(latestVersion);
      message.success(
        t(`${ns}.ytdlp.checkLatestSuccess`, { version: latestVersion }),
      );
    } catch (error) {
      message.error(
        t(`${ns}.ytdlp.checkLatestFailed`, { error: getErrorMessage(error) }),
      );
    } finally {
      setIsCheckingLatest(false);
    }
  }, [message, t, ytdlpStatusQuery]);

  // ── Providers & auth settings ───────────────────────────────────────────
  const providersQuery = api.videoOnlineMedia.providers.useQuery();
  const authSettingsQuery = api.videoOnlineMedia.authSettings.useQuery();
  const updateAuthSettingMutation =
    api.videoOnlineMedia.updateAuthSetting.useMutation();

  const providers = providersQuery.data?.providers ?? [];
  const authSettings = authSettingsQuery.data ?? [];

  // Build initial map from auth settings
  const initialCookies = useMemo(() => {
    const map = new Map<string, string>();
    for (const setting of authSettings) {
      map.set(setting.providerId, setting.cookie ?? "");
    }
    return map;
  }, [authSettings]);

  const [localCookies, setLocalCookies] = useState<Map<string, string>>(
    new Map(),
  );
  const [isSaving, setIsSaving] = useState(false);

  // Initialize local state when auth settings load
  useEffect(() => {
    setLocalCookies(new Map(initialCookies));
  }, [initialCookies]);

  const isDirty = useMemo(() => {
    if (localCookies.size !== initialCookies.size) return true;
    for (const [key, val] of localCookies) {
      if ((initialCookies.get(key) ?? "") !== val) return true;
    }
    return false;
  }, [localCookies, initialCookies]);

  const handleCookieChange = useCallback(
    (providerId: string, value: string) => {
      setLocalCookies((prev) => new Map(prev).set(providerId, value));
    },
    [],
  );

  const handleReset = useCallback(() => {
    setLocalCookies(new Map(initialCookies));
  }, [initialCookies]);

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      const dirty: LocalCookie[] = [];
      for (const [providerId, cookie] of localCookies) {
        if ((initialCookies.get(providerId) ?? "") !== cookie) {
          dirty.push({ providerId, cookie });
        }
      }

      for (const { providerId, cookie } of dirty) {
        const setting = authSettings.find((s) => s.providerId === providerId);
        await updateAuthSettingMutation.mutateAsync({
          provider: providerId,
          displayName: setting?.displayName,
          cookie: cookie.trim(),
          isEnabled: setting?.isEnabled ?? true,
        });
      }

      message.success(t(`${ns}.cookies.saved`));
      const refetched = await authSettingsQuery.refetch();
      if (refetched.data) {
        const freshMap = new Map<string, string>();
        for (const setting of refetched.data) {
          freshMap.set(setting.providerId, setting.cookie ?? "");
        }
        setLocalCookies(freshMap);
      }
    } catch (error) {
      message.error(
        t(`${ns}.cookies.saveFailed`, { error: getErrorMessage(error) }),
      );
    } finally {
      setIsSaving(false);
    }
  }, [
    localCookies,
    initialCookies,
    authSettings,
    updateAuthSettingMutation,
    authSettingsQuery,
    message,
    t,
  ]);

  // ── Render ────────────────────────────────────────────────────────────────

  const isLoading = authSettingsQuery.isLoading;
  const hasError = authSettingsQuery.isError;

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto px-6 py-4 space-y-6">
        {/* yt-dlp Section */}
        <SettingGroup
          title={t(`${ns}.ytdlp.title`)}
          desc={t(`${ns}.ytdlp.desc`)}
        >
          {ytdlpStatusQuery.isLoading && (
            <div className="flex items-center gap-2 text-fg-muted">
              <Spin size="small" />
              <span>{t(`${ns}.ytdlp.checking`)}</span>
            </div>
          )}

          {ytdlpStatusQuery.isError && (
            <Alert
              type="error"
              showIcon
              message={t(`${ns}.ytdlp.checkFailed`)}
              description={getErrorMessage(ytdlpStatusQuery.error)}
            />
          )}

          {ytdlpStatus && (
            <div className="space-y-3">
              <SettingRow
                label={t(`${ns}.ytdlp.status`)}
                orientation="horizontal"
              >
                <div className="flex items-center gap-2">
                  {ytdlpStatus.installed ? (
                    <>
                      <CheckCircle size={16} className="text-success" />
                      <span className="text-success">
                        {t(`${ns}.ytdlp.installed`)}
                      </span>
                    </>
                  ) : (
                    <>
                      <XCircle size={16} className="text-error" />
                      <span className="text-error">
                        {t(`${ns}.ytdlp.notInstalled`)}
                      </span>
                    </>
                  )}
                </div>
              </SettingRow>

              {(ytdlpStatus.installed || ytdlpStatus.version) && (
                <SettingRow
                  label={t(`${ns}.ytdlp.version`)}
                  orientation="horizontal"
                >
                  <span className="text-fg-base">
                    {ytdlpStatus.version ?? t(`${ns}.ytdlp.unknown`)}
                  </span>
                </SettingRow>
              )}

              {(ytdlpStatus.installed || ytdlpStatus.path) && (
                <SettingRow
                  label={t(`${ns}.ytdlp.path`)}
                  orientation="horizontal"
                >
                  <code className="text-xs text-fg-muted font-mono">
                    {ytdlpStatus.path ?? t(`${ns}.ytdlp.unknown`)}
                  </code>
                </SettingRow>
              )}

              <SettingRow
                label={t(`${ns}.ytdlp.latestVersion`)}
                orientation="horizontal"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-fg-muted">
                    {displayLatestVersion ?? t(`${ns}.ytdlp.unknown`)}
                  </span>
                  {isYtdlpAlreadyLatest && (
                    <Tag color="success" size="small">
                      {t(`${ns}.ytdlp.alreadyLatest`)}
                    </Tag>
                  )}
                  <Button
                    loading={isCheckingLatest}
                    onClick={() => void handleCheckLatestVersion()}
                  >
                    {t(`${ns}.ytdlp.checkLatest`)}
                  </Button>
                </div>
              </SettingRow>

              <SettingRow
                label={t(`${ns}.ytdlp.update`)}
                orientation="horizontal"
              >
                <Button
                  loading={updateYtdlpMutation.isPending}
                  onClick={() => void updateYtdlpMutation.mutateAsync()}
                >
                  {t(`${ns}.ytdlp.update`)}
                </Button>
              </SettingRow>
            </div>
          )}
        </SettingGroup>

        {/* Site Cookies Section */}
        <SettingGroup
          title={t(`${ns}.cookies.title`)}
          desc={t(`${ns}.cookies.desc`)}
        >
          {isLoading && (
            <div className="flex items-center gap-2 text-fg-muted">
              <Spin size="small" />
              <span>{t(`${ns}.cookies.loading`)}</span>
            </div>
          )}

          {hasError && (
            <Alert
              type="error"
              showIcon
              message={t(`${ns}.cookies.loadFailed`)}
            />
          )}

          {!isLoading && !hasError && authSettings.length === 0 && (
            <Alert type="info" message={t(`${ns}.cookies.empty`)} />
          )}

          {!isLoading && !hasError && authSettings.length > 0 && (
            <div className="space-y-4">
              {authSettings.map((setting) => {
                const provider = providers.find(
                  (p) => p.id === setting.providerId,
                );
                const localCookie = localCookies.get(setting.providerId) ?? "";
                const sourceSites = provider?.commonSourceSites.join(", ");
                const descParts: string[] = [];

                if (provider?.requiresAuth) {
                  descParts.push(t(`${ns}.cookies.requiresAuth`));
                }
                if (provider?.authConfigurable) {
                  descParts.push(t(`${ns}.cookies.configurable`));
                }
                if (sourceSites) {
                  descParts.push(
                    t(`${ns}.cookies.sources`, { sites: sourceSites }),
                  );
                }

                return (
                  <SettingRow
                    key={setting.providerId}
                    label={
                      <div className="flex items-center gap-2">
                        <span>{setting.displayName ?? setting.providerId}</span>
                        {provider?.requiresAuth && (
                          <Tag color="warning" size="small">
                            {t(`${ns}.cookies.authRequired`)}
                          </Tag>
                        )}
                      </div>
                    }
                    desc={descParts.join(" · ")}
                    orientation="vertical"
                  >
                    <Input.TextArea
                      value={localCookie}
                      onChange={(e) =>
                        handleCookieChange(setting.providerId, e.target.value)
                      }
                      rows={3}
                      placeholder={t(`${ns}.cookies.placeholder`)}
                      className="font-mono text-xs"
                    />
                    {setting.updatedAt && (
                      <div className="text-xs text-fg-muted mt-1">
                        {t(`${ns}.cookies.lastUpdated`, {
                          time: new Date(setting.updatedAt).toLocaleString(),
                        })}
                      </div>
                    )}
                  </SettingRow>
                );
              })}
            </div>
          )}
        </SettingGroup>
      </div>

      {/* Footer */}
      <StickySaveBar
        dirty={isDirty}
        loading={isSaving}
        onSave={() => void handleSave()}
        onReset={handleReset}
        saveLabel={t(`${ns}.save`)}
        resetLabel={t(`${ns}.reset`)}
        message={t(`${ns}.unsavedChanges`)}
      />
    </div>
  );
}

export default function DownloadEngineSettingsWindow({
  win,
}: {
  win: ShellWindowHandle;
}) {
  void win;

  return (
    <I18nextProvider i18n={i18n}>
      <ConfigProvider>
        <ToastProvider>
          <QueryClientProvider client={queryClient}>
            <DownloadEngineSettingsContent />
          </QueryClientProvider>
        </ToastProvider>
      </ConfigProvider>
    </I18nextProvider>
  );
}
