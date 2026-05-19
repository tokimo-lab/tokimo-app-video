import type { FormInstance, TemplateVariable } from "@tokimo/ui";
import {
  Checkbox,
  cn,
  Form,
  Popover,
  QuestionCircleOutlined,
  Select,
  TemplateInput,
  useWatch,
} from "@tokimo/ui";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  getDefaultFileFormat,
  getDefaultFolderFormat,
  getVarsForType,
  ORGANIZE_LANG_OPTIONS,
  type PlaceholderItem,
  renderTemplate,
} from "./organize-constants";
import { getOrganizeSample } from "./organize-samples";

// TODO(phase-6): useAdultMode - video sidecar does not have generalSettings endpoint yet
function useAdultMode() {
  return { enabled: false };
}

const JINJA2_DOC_URL = "https://jinja.palletsprojects.com/en/3.1.x/templates/";

function useTemplateVars(
  items: PlaceholderItem[],
  t: (key: string) => string,
): TemplateVariable[] {
  return useMemo(
    () =>
      items.map((item) => ({
        key: item.key,
        label: t(`media.organizingSettings.placeholders.${item.descKey}`),
      })),
    [items, t],
  );
}

function TemplatePreview({
  form,
  sampleData,
  seasonFolder,
  defaultFolder,
  defaultFile,
  contentType,
}: {
  form: FormInstance;
  sampleData: Record<string, string>;
  seasonFolder?: string;
  defaultFolder: string;
  defaultFile: string;
  contentType: string;
}) {
  const { t } = useTranslation();
  const folderTpl = useWatch<string>("folderFormat", form) || defaultFolder;
  const fileTpl = useWatch<string>("fileFormat", form) || defaultFile;

  const ext = contentType === "photo" ? ".jpg" : ".mkv";
  const folder = renderTemplate(folderTpl, sampleData);
  const file = renderTemplate(fileTpl, sampleData);
  const parts = [folder, seasonFolder, file ? `${file}${ext}` : ""].filter(
    Boolean,
  );
  const preview = parts.join("/");

  if (!preview) return null;

  return (
    <div className="text-xs text-fg-muted mt-1 flex items-baseline gap-1.5">
      <span className="shrink-0">
        {t("media.organizingSettings.previewOutput")}
      </span>
      <code className="text-[var(--accent-text)] break-all">{preview}</code>
    </div>
  );
}

function PlaceholderHelp({ items }: { items: PlaceholderItem[] }) {
  const { t } = useTranslation();
  return (
    <span className="text-xs text-fg-muted inline-flex items-center gap-2 mb-2">
      <span>
        {t("media.organizingSettings.placeholders.descPre")}
        <a
          className="text-[var(--accent-text)] hover:text-[var(--accent)]"
          href={JINJA2_DOC_URL}
          target="_blank"
          rel="noopener noreferrer"
        >
          {t("media.organizingSettings.placeholders.descLink")}
        </a>
        {t("media.organizingSettings.placeholders.descPost")}
      </span>
      <Popover
        placement="bottomLeft"
        trigger="hover"
        content={
          <div className="text-xs max-w-sm">
            <table className="border-collapse">
              <tbody>
                {items.map((item) => (
                  <tr key={item.key}>
                    <td className="pr-3 py-0.5 whitespace-nowrap align-top">
                      <code className="text-[var(--accent-text)]">
                        {`{{${item.key}}}`}
                      </code>
                    </td>
                    <td className="py-0.5 text-fg-secondary">
                      {t(
                        `media.organizingSettings.placeholders.${item.descKey}`,
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        }
      >
        <span className="cursor-help inline-flex items-center gap-1 text-[var(--accent-text)] hover:text-[var(--accent)]">
          <QuestionCircleOutlined className="w-3.5 h-3.5" />
          {t("media.organizingSettings.placeholders.varsLabel")}
        </span>
      </Popover>
    </span>
  );
}

export default function VideoOrganizeFields({ form }: { form: FormInstance }) {
  const { t } = useTranslation();
  const appType: string | undefined = useWatch("type", form);
  const organizeLang: string | undefined = useWatch("organizeLang", form);
  const adultModeEnabled = useAdultMode().enabled;
  const prevType = useRef<string | undefined>(appType);

  useEffect(() => {
    if (!appType || appType === prevType.current) return;
    const prev = prevType.current;
    prevType.current = appType;

    const oldFolderDefault = prev ? getDefaultFolderFormat(prev) : "";
    const oldFileDefault = prev ? getDefaultFileFormat(prev) : "";
    const curFolder = (form.getFieldValue("folderFormat") as string) ?? "";
    const curFile = (form.getFieldValue("fileFormat") as string) ?? "";

    if (!curFolder || curFolder === oldFolderDefault) {
      form.setFieldValue("folderFormat", getDefaultFolderFormat(appType));
    }
    if (!curFile || curFile === oldFileDefault) {
      form.setFieldValue("fileFormat", getDefaultFileFormat(appType));
    }
  }, [appType, form]);

  const vars = getVarsForType(appType ?? "movie");
  const templateVars = useTemplateVars(vars, t);
  const effectiveLang = organizeLang || "zh-CN";
  const sampleData = useMemo(
    () => getOrganizeSample(appType ?? "movie", effectiveLang),
    [appType, effectiveLang],
  );
  const defaultFolder = getDefaultFolderFormat(appType ?? "movie");
  const defaultFile = getDefaultFileFormat(appType ?? "movie");

  if (!appType) return null;

  const isAdult = appType === "adult";
  const isMovie = appType === "movie" || appType === "documentary";
  const isTvLike =
    appType === "tv" || appType === "anime" || appType === "variety";
  const showOrganizeLang = isMovie || isTvLike || isAdult;
  const showDiscSettings = isMovie || isTvLike;
  const showStrictYear = appType === "movie";

  if (isAdult && !adultModeEnabled) return null;

  const seasonFolder = isTvLike ? "Season 1" : undefined;

  return (
    <div className={cn("space-y-2")}>
      <Form.Item
        name="linkMode"
        label={t("media.mediaFolders.linkMode")}
        rules={[{ required: true }]}
        extra={t("media.mediaFolders.linkModeExtra")}
      >
        <Select>
          <Select.Option value="hardlink">
            {t("media.mediaFolders.linkModes.hardlink")}
          </Select.Option>
          <Select.Option value="softlink">
            {t("media.mediaFolders.linkModes.softlink")}
          </Select.Option>
          <Select.Option value="copy">
            {t("media.mediaFolders.linkModes.copy")}
          </Select.Option>
          <Select.Option value="move">
            {t("media.mediaFolders.linkModes.move")}
          </Select.Option>
        </Select>
      </Form.Item>

      <PlaceholderHelp items={vars} />

      <Form.Item
        name="folderFormat"
        label={t("media.organizingSettings.folderFormat")}
      >
        <TemplateInput placeholder={defaultFolder} vars={templateVars} />
      </Form.Item>

      <Form.Item
        name="fileFormat"
        label={t("media.organizingSettings.fileNameFormat")}
      >
        <TemplateInput placeholder={defaultFile} vars={templateVars} />
      </Form.Item>

      <TemplatePreview
        form={form}
        sampleData={sampleData}
        seasonFolder={seasonFolder}
        defaultFolder={defaultFolder}
        defaultFile={defaultFile}
        contentType={appType}
      />

      {showOrganizeLang && (
        <Form.Item
          name="organizeLang"
          label={t("media.organizingSettings.organizeLang")}
          extra={t("media.organizingSettings.organizeLangDesc")}
        >
          <Select allowClear placeholder="zh-CN">
            {ORGANIZE_LANG_OPTIONS.map((opt) => (
              <Select.Option key={opt.value} value={opt.value}>
                {opt.label}
              </Select.Option>
            ))}
          </Select>
        </Form.Item>
      )}

      {showDiscSettings && (
        <div className="space-y-2 pt-1">
          <Form.Item
            name="flattenDisc"
            valuePropName="checked"
            className="!mb-0"
          >
            <Checkbox>{t("media.organizingSettings.flattenDisc")}</Checkbox>
          </Form.Item>

          <Form.Item
            name="fixEmbyDisc"
            valuePropName="checked"
            className="!mb-0"
          >
            <Checkbox>{t("media.organizingSettings.fixEmbyDisc")}</Checkbox>
          </Form.Item>
        </div>
      )}

      {showStrictYear && (
        <Form.Item
          name="strictYearMatch"
          valuePropName="checked"
          className="!mb-0"
        >
          <Checkbox>{t("media.organizingSettings.strictYearMatch")}</Checkbox>
        </Form.Item>
      )}
    </div>
  );
}
