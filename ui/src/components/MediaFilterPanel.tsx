import { cn } from "@tokimo/ui";
import { X } from "lucide-react";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";

// ── Country code → display name mapping ──────────────────────────────────────

const COUNTRY_NAMES: Record<string, string> = {
  AL: "阿尔巴尼亚",
  AR: "阿根廷",
  AT: "奥地利",
  AU: "澳大利亚",
  BE: "比利时",
  BG: "保加利亚",
  BR: "巴西",
  CA: "加拿大",
  CH: "瑞士",
  CL: "智利",
  CN: "中国大陆",
  CO: "哥伦比亚",
  CZ: "捷克",
  DE: "德国",
  DK: "丹麦",
  EG: "埃及",
  ES: "西班牙",
  FI: "芬兰",
  FR: "法国",
  GB: "英国",
  GR: "希腊",
  HK: "中国香港",
  HU: "匈牙利",
  ID: "印度尼西亚",
  IE: "爱尔兰",
  IL: "以色列",
  IN: "印度",
  IR: "伊朗",
  IT: "意大利",
  JP: "日本",
  KR: "韩国",
  MA: "摩洛哥",
  MO: "中国澳门",
  MX: "墨西哥",
  MY: "马来西亚",
  NL: "荷兰",
  NO: "挪威",
  NZ: "新西兰",
  PH: "菲律宾",
  PK: "巴基斯坦",
  PL: "波兰",
  PT: "葡萄牙",
  RO: "罗马尼亚",
  RU: "俄罗斯",
  SA: "沙特",
  SE: "瑞典",
  SG: "新加坡",
  TH: "泰国",
  TR: "土耳其",
  TW: "中国台湾",
  UA: "乌克兰",
  US: "美国",
  VN: "越南",
  ZA: "南非",
};

export function getCountryDisplayName(code: string): string {
  return COUNTRY_NAMES[code] ?? code;
}

// ── Types ────────────────────────────────────────────────────────────────────

export interface FilterOption {
  label: string;
  value: string;
}

export interface FilterRow {
  key: string;
  label: string;
  options: readonly FilterOption[];
}

export interface MediaFilters {
  sortBy: string;
  genreId: string;
  country: string;
  runtime: string;
  favorite: string;
  resolution: string;
}

export const EMPTY_FILTERS: MediaFilters = {
  sortBy: "",
  genreId: "",
  country: "",
  runtime: "",
  favorite: "",
  resolution: "",
};

// ── Constants ────────────────────────────────────────────────────────────────

const SORT_OPTIONS: FilterOption[] = [
  { label: "settings.library.sortAddedAt", value: "addedAt" },
  { label: "settings.library.sortTitleAsc", value: "title_asc" },
  { label: "settings.library.sortTitleDesc", value: "title_desc" },
  { label: "settings.library.sortYearDesc", value: "year_desc" },
  { label: "settings.library.sortYearAsc", value: "year_asc" },
  { label: "settings.library.sortRating", value: "rating" },
];

const RUNTIME_OPTIONS: FilterOption[] = [
  { label: "media.video.filter.runtimeShort", value: "short" },
  { label: "media.video.filter.runtimeMedium", value: "medium" },
  { label: "media.video.filter.runtimeLong", value: "long" },
  { label: "media.video.filter.runtimeExtraLong", value: "extra_long" },
];

const FAVORITE_OPTIONS: FilterOption[] = [
  { label: "media.video.filter.favoriteOnly", value: "true" },
];

const RESOLUTION_OPTIONS: FilterOption[] = [
  { label: "4K", value: "4k" },
  { label: "1080P", value: "1080p" },
  { label: "720P", value: "720p" },
  { label: "480P", value: "480p" },
];

// ── Pill ─────────────────────────────────────────────────────────────────────

function FilterPill({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "cursor-pointer whitespace-nowrap rounded-md px-3 py-1 text-[13px] font-medium transition-colors",
        active
          ? "bg-[var(--accent)] text-white"
          : "text-fg-secondary hover:text-fg-primary",
      )}
    >
      {label}
    </button>
  );
}

// ── Component ────────────────────────────────────────────────────────────────

interface MediaFilterPanelProps {
  filters: MediaFilters;
  onChange: (filters: MediaFilters) => void;
  genreOptions: readonly FilterOption[];
  countryOptions: readonly FilterOption[];
  /** Whether this is movie (has runtime) or TV */
  showRuntime?: boolean;
}

export default function MediaFilterPanel({
  filters,
  onChange,
  genreOptions,
  countryOptions,
  showRuntime = false,
}: MediaFilterPanelProps) {
  const { t } = useTranslation();

  const handleChange = useCallback(
    (key: keyof MediaFilters, value: string) => {
      const next = { ...filters, [key]: filters[key] === value ? "" : value };
      onChange(next);
    },
    [filters, onChange],
  );

  const activeCount = useMemo(() => {
    let c = 0;
    if (filters.sortBy && filters.sortBy !== "addedAt") c++;
    if (filters.genreId) c++;
    if (filters.country) c++;
    if (filters.runtime) c++;
    if (filters.favorite) c++;
    if (filters.resolution) c++;
    return c;
  }, [filters]);

  const handleClear = useCallback(() => {
    onChange(EMPTY_FILTERS);
  }, [onChange]);

  const rows: FilterRow[] = useMemo(() => {
    const r: FilterRow[] = [
      {
        key: "sortBy",
        label: t("media.video.filter.sort"),
        options: SORT_OPTIONS,
      },
    ];
    if (genreOptions.length > 0) {
      r.push({
        key: "genreId",
        label: t("media.video.filter.genre"),
        options: genreOptions,
      });
    }
    if (countryOptions.length > 0) {
      r.push({
        key: "country",
        label: t("media.video.filter.region"),
        options: countryOptions,
      });
    }
    if (showRuntime) {
      r.push({
        key: "runtime",
        label: t("media.video.filter.runtime"),
        options: RUNTIME_OPTIONS,
      });
    }
    r.push({
      key: "favorite",
      label: t("media.video.filter.favorite"),
      options: FAVORITE_OPTIONS,
    });
    r.push({
      key: "resolution",
      label: t("media.video.filter.resolution"),
      options: RESOLUTION_OPTIONS,
    });
    return r;
  }, [genreOptions, countryOptions, showRuntime, t]);

  return (
    <div className="space-y-1">
      {rows.map((row) => {
        const isRowActive = !!filters[row.key as keyof MediaFilters];
        return (
          <div key={row.key} className="flex items-start gap-2 py-1.5">
            <span
              className={cn(
                "w-14 shrink-0 pt-1 text-[13px] font-semibold",
                isRowActive ? "text-[var(--accent)]" : "text-fg-secondary",
              )}
            >
              {row.label}
            </span>
            <div className="flex flex-wrap items-center gap-1">
              <FilterPill
                label={t("media.video.filter.all")}
                active={!filters[row.key as keyof MediaFilters]}
                onClick={() => handleChange(row.key as keyof MediaFilters, "")}
              />
              {row.options.map((opt) => (
                <FilterPill
                  key={opt.value}
                  label={
                    opt.label.includes(".")
                      ? t(opt.label as Parameters<typeof t>[0])
                      : opt.label
                  }
                  active={filters[row.key as keyof MediaFilters] === opt.value}
                  onClick={() =>
                    handleChange(row.key as keyof MediaFilters, opt.value)
                  }
                />
              ))}
            </div>
          </div>
        );
      })}

      {/* Clear filters */}
      {activeCount > 0 && (
        <div className="flex items-center justify-end pt-1">
          <button
            type="button"
            onClick={handleClear}
            className="flex cursor-pointer items-center gap-1 text-xs text-fg-muted hover:text-fg-primary"
          >
            <X className="h-3 w-3" />
            {t("media.search.clearFilters")}
          </button>
        </div>
      )}
    </div>
  );
}
