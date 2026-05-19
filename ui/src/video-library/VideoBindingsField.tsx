import {
  Button,
  cn,
  DeleteOutlined,
  Form,
  Input,
  PlusOutlined,
  Tag,
} from "@tokimo/ui";
import { CheckCircle, FolderOpen, HardDrive } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { VfsDto } from "../api/types";
import StorageBindingForm from "./StorageBindingForm";

export type VideoBinding = {
  _key: number;
  sourceId: string;
  rootPath: string;
  isDefaultDownload: boolean;
};

function BindingCard({
  binding,
  index,
  sources,
  onUpdate,
  onRemove,
  onSetDefault,
  hideDefaultToggle = false,
}: {
  binding: VideoBinding;
  index: number;
  sources: VfsDto[];
  onUpdate: (index: number, patch: Partial<VideoBinding>) => void;
  onRemove: (index: number) => void;
  onSetDefault: (index: number) => void;
  hideDefaultToggle?: boolean;
}) {
  return (
    <div
      className={cn(
        "rounded-xl border p-4 transition-all",
        binding.isDefaultDownload && !hideDefaultToggle
          ? "border-green-300 dark:border-green-700 bg-green-50/40 dark:bg-green-950/15"
          : "border-border-base bg-white/50 dark:bg-white/[0.02]",
      )}
    >
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2">
          <Tag color="success" className="text-xs">
            文件来源
          </Tag>
          {!hideDefaultToggle && binding.isDefaultDownload && (
            <span className="flex items-center gap-1 text-xs text-green-600 dark:text-green-400">
              <CheckCircle className="w-3 h-3" />
              默认下载位置
            </span>
          )}
        </div>
        <Button
          variant="text"
          danger
          size="small"
          icon={<DeleteOutlined />}
          onClick={() => onRemove(index)}
        />
      </div>

      <StorageBindingForm
        sources={sources}
        value={{ sourceId: binding.sourceId, path: binding.rootPath }}
        onChange={({ sourceId, path }) =>
          onUpdate(index, { sourceId, rootPath: path })
        }
      />

      {!hideDefaultToggle && !binding.isDefaultDownload && (
        <div className="mt-3 pt-3 border-t border-border-base">
          <button
            type="button"
            onClick={() => onSetDefault(index)}
            className="text-xs text-fg-muted hover:text-green-600 dark:hover:text-green-400 transition-colors cursor-pointer"
          >
            设为默认下载位置
          </button>
        </div>
      )}
    </div>
  );
}

export default function VideoBindingsField({
  sources,
  form,
  initialSources,
  maxBindings,
}: {
  sources: VfsDto[];
  form: ReturnType<typeof Form.useForm>[0];
  initialSources?: Array<{
    sourceId: string;
    rootPath: string;
    isDefaultDownload?: boolean | null;
  }>;
  maxBindings?: number;
}) {
  const nextKeyRef = useRef(0);
  const [bindings, setBindings] = useState<VideoBinding[]>([]);
  const initializedRef = useRef(false);

  const sync = useCallback(
    (next: VideoBinding[]) => {
      setBindings(next);
      form.setFieldValue("bindings", next);
    },
    [form],
  );

  useEffect(() => {
    if (initializedRef.current) return;
    initializedRef.current = true;
    const items: VideoBinding[] = (initialSources ?? []).map((s) => ({
      _key: nextKeyRef.current++,
      sourceId: s.sourceId,
      rootPath: s.rootPath,
      isDefaultDownload: s.isDefaultDownload ?? false,
    }));
    sync(items);
  }, [initialSources, sync]);

  const add = () => {
    const hasDefault = bindings.some((b) => b.isDefaultDownload);
    sync([
      ...bindings,
      {
        _key: nextKeyRef.current++,
        sourceId: "",
        rootPath: "",
        isDefaultDownload: !hasDefault,
      },
    ]);
  };

  const remove = (index: number) =>
    sync(bindings.filter((_, i) => i !== index));

  const update = (index: number, patch: Partial<VideoBinding>) => {
    const next = [...bindings];
    next[index] = { ...next[index], ...patch };
    if ("sourceId" in patch) next[index].rootPath = "";
    sync(next);
  };

  const setDefaultDownload = (index: number) => {
    sync(bindings.map((b, i) => ({ ...b, isDefaultDownload: i === index })));
  };

  const isSingleMode = maxBindings === 1;
  const canAdd =
    typeof maxBindings === "number" ? bindings.length < maxBindings : true;

  return (
    <div className="space-y-6">
      <Form.Item name="bindings" hidden>
        <Input />
      </Form.Item>

      <div className="rounded-2xl border border-border-base bg-surface-base/50 dark:bg-white/[0.02] p-5">
        <div className="flex items-start gap-3 mb-4">
          <div className="flex items-center justify-center w-9 h-9 rounded-lg shrink-0 bg-green-500/10 text-green-600 dark:bg-green-500/20 dark:text-green-400">
            <HardDrive className="w-[18px] h-[18px]" />
          </div>
          <div className="min-w-0">
            <h4 className="text-sm font-semibold text-fg-primary">文件来源</h4>
            <p className="text-xs text-fg-muted mt-0.5">
              连接本地磁盘、NAS
              或远程存储路径，系统将自动扫描并导入其中的媒体文件
            </p>
          </div>
        </div>

        {bindings.length > 0 && (
          <div className="space-y-3 mb-4">
            {bindings.map((binding, index) => (
              <BindingCard
                key={binding._key}
                binding={binding}
                index={index}
                sources={sources}
                onUpdate={update}
                onRemove={remove}
                onSetDefault={setDefaultDownload}
                hideDefaultToggle={isSingleMode}
              />
            ))}
          </div>
        )}

        {bindings.length === 0 ? (
          <button
            type="button"
            onClick={add}
            className="w-full flex flex-col items-center justify-center gap-2 py-8 rounded-xl border-2 border-dashed border-border-base hover:border-green-300 dark:hover:border-green-700 hover:bg-green-50/50 dark:hover:bg-green-950/10 transition-all cursor-pointer group"
          >
            <div className="flex items-center justify-center w-10 h-10 rounded-full bg-fill-tertiary group-hover:bg-green-100 dark:group-hover:bg-green-900/30 transition-colors">
              <FolderOpen className="w-5 h-5 text-fg-muted group-hover:text-green-500 transition-colors" />
            </div>
            <span className="text-sm text-fg-muted group-hover:text-green-600 dark:group-hover:text-green-400 transition-colors">
              添加文件来源
            </span>
          </button>
        ) : canAdd ? (
          <Button
            variant="text"
            size="small"
            icon={<PlusOutlined />}
            onClick={add}
          >
            添加文件来源
          </Button>
        ) : null}
      </div>
    </div>
  );
}
