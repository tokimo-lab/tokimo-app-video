import { Input, Select } from "@tokimo/ui";
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
            存储源
          </div>
          <Select
            className="w-full"
            options={sources.map((s) => ({
              label: `${s.name} (${s.type})`,
              value: s.id,
            }))}
            value={value.sourceId || undefined}
            onChange={(v) => handleSourceChange(v as string)}
            placeholder="选择存储源"
            disabled={disabled}
          />
        </div>
      )}
      <div>
        <div className="block text-xs font-medium text-fg-muted mb-1">路径</div>
        {value.sourceId ? (
          <Input
            placeholder="/mnt/media/"
            value={value.path}
            onChange={handlePathChange}
            disabled={disabled}
          />
        ) : (
          <Input placeholder="请先选择存储源" disabled />
        )}
      </div>
    </div>
  );
}
