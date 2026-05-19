import { cn } from "@tokimo/ui";
import { X } from "lucide-react";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";

// ── Country code → display name mapping ──────────────────────────────────────

const COUNTRY_NAMES: Record<string, string> = {
  AL: "media.countries.AL",
  AR: "media.countries.AR",
  AT: "media.countries.AT",
  AU: "media.countries.AU",
  BE: "media.countries.BE",
  BG: "media.countries.BG",
  BR: "media.countries.BR",
  CA: "media.countries.CA",
  CH: "media.countries.CH",
  CL: "media.countries.CL",
  CN: "media.countries.CN",
  CO: "media.countries.CO",
  CZ: "media.countries.CZ",
  DE: "media.countries.DE",
  DK: "media.countries.DK",
  EG: "media.countries.EG",
  ES: "media.countries.ES",
  FI: "media.countries.FI",
  FR: "media.countries.FR",
  GB: "media.countries.GB",
  GR: "media.countries.GR",
  HK: "media.countries.HK",
  HU: "media.countries.HU",
  ID: "media.countries.ID",
  IE: "media.countries.IE",
  IL: "media.countries.IL",
  IN: "media.countries.IN",
  IR: "media.countries.IR",
  IT: "media.countries.IT",
  JP: "media.countries.JP",
  KR: "media.countries.KR",
  MA: "media.countries.MA",
  MO: "media.countries.MO",
  MX: "media.countries.MX",
  MY: "media.countries.MY",
  NL: "media.countries.NL",
  NO: "media.countries.NO",
  NZ: "media.countries.NZ",
  PH: "media.countries.PH",
  PK: "media.countries.PK",
  PL: "media.countries.PL",
  PT: "media.countries.PT",
  RO: "media.countries.RO",
  RU: "media.countries.RU",
  SA: "media.countries.SA",
  SE: "media.countries.SE",
  SG: "media.countries.SG",
  TH: "media.countries.TH",
  TR: "media.countries.TR",
  TW: "media.countries.TW",
  UA: "media.countries.UA",
  US: "media.countries.US",
  VN: "media.countries.VN",
  ZA: "media.countries.ZA",
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
