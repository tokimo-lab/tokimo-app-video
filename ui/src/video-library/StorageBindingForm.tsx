import { useShellApi } from "@tokimo/sdk";
import { Button, FolderOpenOutlined, Input, Select } from "@tokimo/ui";
import { useTranslation } from "react-i18next";
import type { VfsDisplayHints, VfsDto } from "../api/types";

const PATH_TYPES = new Set(["local", "nfs", "smb", "webdav", "ftp", "sftp"]);
const BROWSEABLE_CLOUD_TYPES =
  /^(115cloud|aliyundrive|baidu_netdisk|quark|uc|123pan|pikpak|thunder|139yun|189cloud|mopan|wopan|lanzou|google_drive|onedrive|dropbox|mega|terabox|yandex_disk|s3)$/;

const isPathType = (type: string): boolean => PATH_TYPES.has(type);
const isBrowseableType = (type: string): boolean =>
  isPathType(type) || BROWSEABLE_CLOUD_TYPES.test(type);

type RemotePathFieldProps = {
  value?: string;
  onChange?: (v: string) => void;
  placeholder?: string;
  disabled?: boolean;
  sourceId?: string;
  protocolPrefix?: string | null;
  browserInitialPath?: string;
  onSelectTransform?: (path: string) => string;
};

function RemotePathField({
  value,
  onChange,
  placeholder,
  disabled,
  sourceId,
  protocolPrefix,
  browserInitialPath,
  onSelectTransform,
}: RemotePathFieldProps) {
  const { t } = useTranslation();
  const shell = useShellApi();
  const handleBrowse = async () => {
    const picked = await shell.pickFilePath({
      sourceId,
      initialPath: browserInitialPath ?? value?.trim() ?? "/",
      protocolPrefix: protocolPrefix ?? undefined,
      title: t("media.videoBindings.browseDirectory"),
      width: 600,
      height: 480,
    });
    if (picked != null)
      onChange?.(onSelectTransform ? onSelectTransform(picked) : picked);
  };
  return (
    <div className="flex flex-1 min-w-0 rounded-md border border-black/[0.08] dark:border-white/[0.1] focus-within:border-[var(--color-accent)] focus-within:ring-1 focus-within:ring-[var(--color-accent)] transition-colors">
      <Input
        className="flex-1 min-w-0 !rounded-r-none !border-0 !ring-0 focus-within:!border-0 focus-within:!ring-0"
        placeholder={placeholder}
        value={value ?? ""}
        onChange={(e) => onChange?.(e.target.value)}
        disabled={disabled}
      />
      <Button
        className="!rounded-l-none !border-0 !border-l !border-l-black/[0.08] dark:!border-l-white/[0.1]"
        icon={<FolderOpenOutlined />}
        title={t("media.videoBindings.browseDirectory")}
        onClick={handleBrowse}
        disabled={disabled}
      />
    </div>
  );
}

type RootPathFieldProps = {
  sourceId: string;
  sourceType: string;
  displayHints: VfsDisplayHints | null | undefined;
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
};

function RootPathField({
  sourceId,
  sourceType,
  displayHints,
  value,
  onChange,
  disabled,
}: RootPathFieldProps) {
  const localRoot =
    sourceType === "local"
      ? displayHints?.rootPath?.trim() || undefined
      : undefined;
  const toLocal = (raw: string, root: string) => {
    const trimmed = raw.trim();
    if (!trimmed || trimmed === root) return "/";
    return trimmed.startsWith(`${root}/`) ? trimmed.slice(root.length) : "/";
  };
  const fromLocal = (p: string, root: string) => {
    const trimmed = p.trim();
    if (!trimmed || trimmed === "/") return root;
    return `${root}${trimmed.startsWith("/") ? trimmed : `/${trimmed}`}`;
  };
  if (isPathType(sourceType)) {
    return (
      <RemotePathField
        value={value}
        onChange={onChange}
        placeholder="/mnt/media/"
        sourceId={sourceId}
        protocolPrefix={displayHints?.protocolPrefix}
        browserInitialPath={localRoot ? toLocal(value, localRoot) : undefined}
        onSelectTransform={
          localRoot ? (p) => fromLocal(p, localRoot) : undefined
        }
        disabled={disabled}
      />
    );
  }
  if (isBrowseableType(sourceType)) {
    return (
      <RemotePathField
        value={value}
        onChange={onChange}
        placeholder="/"
        sourceId={sourceId}
        protocolPrefix={displayHints?.protocolPrefix}
        disabled={disabled}
      />
    );
  }
  return (
    <Input
      placeholder="库 ID / 路径"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled}
    />
  );
}

export type StorageBindingValue = { sourceId: string; path: string };

export type StorageBindingFormProps = {
  sources: VfsDto[];
  value: StorageBindingValue;
  onChange: (next: StorageBindingValue) => void;
  showSourceSelect?: boolean;
  disabled?: boolean;
};

export default function StorageBindingForm({
  sources,
  value,
  onChange,
  showSourceSelect = true,
  disabled = false,
}: StorageBindingFormProps) {
  const { t } = useTranslation();
  const selectedSource = sources.find((s) => s.id === value.sourceId);
  return (
    <div className="space-y-3">
      {showSourceSelect && (
        <div>
          <div className="block text-xs font-medium text-fg-muted mb-1">
            {t("media.videoBindings.storageSource")}
          </div>
          <Select
            className="w-full"
            options={sources.map((s) => ({
              label: `${s.name} (${s.type})`,
              value: s.id,
            }))}
            value={value.sourceId || undefined}
            onChange={(v) => onChange({ sourceId: v as string, path: "" })}
            placeholder={t("media.videoBindings.selectStorageSource")}
            disabled={disabled}
          />
        </div>
      )}
      <div>
        <div className="block text-xs font-medium text-fg-muted mb-1">
          {t("media.videoBindings.path")}
        </div>
        {value.sourceId ? (
          <RootPathField
            sourceId={value.sourceId}
            sourceType={selectedSource?.type ?? ""}
            displayHints={selectedSource?.displayHints}
            value={value.path}
            onChange={(path) => onChange({ ...value, path })}
            disabled={disabled}
          />
        ) : (
          <Input
            placeholder={t("media.videoBindings.selectSourceFirst")}
            disabled
          />
        )}
      </div>
    </div>
  );
}
