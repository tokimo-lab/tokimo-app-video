import { Input, Select } from "@tokimo/ui";
import { useTranslation } from "react-i18next";
import type { VfsDto } from "../api/types";

export interface StorageBindingValue {
  sourceId: string;
  path: string;
}

export interface StorageBindingFormProps {
  sources: VfsDto[];
  value: StorageBindingValue;
  onChange: (next: StorageBindingValue) => void;
  showSourceSelect?: boolean;
  disabled?: boolean;
}

export default function StorageBindingForm({
  sources,
  value,
  onChange,
  showSourceSelect = true,
  disabled = false,
}: StorageBindingFormProps) {
  const { t } = useTranslation();
  const handleSourceChange = (sourceId: string) => {
    onChange({ sourceId, path: "" });
  };

  const handlePathChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onChange({ ...value, path: e.target.value });
  };

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
            onChange={(v) => handleSourceChange(v as string)}
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
          <Input
            placeholder="/mnt/media/"
            value={value.path}
            onChange={handlePathChange}
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
