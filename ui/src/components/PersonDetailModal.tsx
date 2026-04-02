import { useQueryClient } from "@tanstack/react-query";
import { HorizontalScroll, Modal, Spin, useToast } from "@tokiomo/components";
import { Film, RefreshCw } from "lucide-react";
import { useState } from "react";
import {
  PersonPlaceholder,
  SectionTitle,
} from "@/apps/media/pages/media-detail-shared";
import { api } from "@/generated/rust-api";
import { resolveMediaImage } from "@/lib/media-image";
import { useWindowNav } from "@/system";

// --- constants ---

const ROLE_LABELS: Record<string, string> = {
  actor: "演员",
  director: "导演",
  writer: "编剧",
  producer: "制片人",
  composer: "作曲",
  cinematographer: "摄影",
};

const GENDER_LABELS: Record<string, string> = {
  male: "男",
  female: "女",
  "non-binary": "非二元",
  unknown: "未知",
};

const DEPT_LABELS: Record<string, string> = {
  Acting: "演员",
  Directing: "导演",
  Writing: "编剧",
  Production: "制片",
  Sound: "音效",
  Camera: "摄影",
  "Costume & Make-Up": "造型",
  "Visual Effects": "视效",
  Editing: "剪辑",
  Art: "美术",
  Crew: "剧组",
};

interface PersonDetailModalProps {
  personId: string | null;
  character?: string | null;
  onClose: () => void;
}

export default function PersonDetailModal({
  personId,
  character,
  onClose,
}: PersonDetailModalProps) {
  const { navigate } = useWindowNav();
  const toast = useToast();
  const qc = useQueryClient();
  const [bioExpanded, setBioExpanded] = useState(false);

  const { data: person, isLoading } = api.app.getPersonDetail.useQuery(
    { id: personId! },
    { enabled: !!personId },
  );

  const refreshMutation = api.app.refreshPersonFromTmdb.useMutation({
    onSuccess: () => {
      toast.success("已从 TMDB 更新演员数据");
      api.app.getPersonDetail.invalidate(qc, { id: personId! });
    },
    onError: (err) => {
      toast.error(err.message || "从 TMDB 获取数据失败");
    },
  });

  const profileSrc = person
    ? resolveMediaImage(person.profileKey, person.profilePath)
    : null;

  const grouped = (person?.credits ?? []).reduce<
    Record<string, NonNullable<typeof person>["credits"]>
  >((acc, c) => {
    const key = c?.role ?? "other";
    if (!acc[key]) acc[key] = [];
    acc[key]!.push(c);
    return acc;
  }, {});

  const hasCreditsInLibrary = (person?.credits ?? []).some(
    (c) => c?.mediaTitle,
  );

  const deptLabel = person?.knownForDepartment
    ? (DEPT_LABELS[person.knownForDepartment] ?? person.knownForDepartment)
    : null;

  const metaParts: string[] = [];
  if (character) metaParts.push(`饰 ${character}`);
  if (deptLabel) metaParts.push(deptLabel);
  if (person?.gender && GENDER_LABELS[person.gender])
    metaParts.push(GENDER_LABELS[person.gender]);
  if (person?.birthday) {
    const dates = person.deathday
      ? `${person.birthday} — ${person.deathday}`
      : person.birthday;
    metaParts.push(dates);
  } else if (person?.deathday) {
    metaParts.push(`† ${person.deathday}`);
  }
  if (person?.birthplace) metaParts.push(person.birthplace);

  // Modal header title: mini avatar + name + meta
  const modalTitle = person ? (
    <div className="flex items-center gap-3 min-w-0">
      <div className="h-10 w-7 flex-shrink-0 overflow-hidden rounded-md shadow ring-1 ring-black/10 dark:ring-white/10">
        {profileSrc ? (
          <img
            src={profileSrc}
            alt={person.name}
            className="h-full w-full object-cover"
          />
        ) : (
          <div className="flex h-full w-full items-center justify-center bg-[var(--bg-skeleton)]">
            <PersonPlaceholder />
          </div>
        )}
      </div>
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate font-semibold text-[var(--text-primary)]">
            {person.name}
          </span>
          {person.originalName && person.originalName !== person.name && (
            <span className="hidden truncate text-xs font-normal text-muted-foreground sm:block">
              {person.originalName}
            </span>
          )}
        </div>
        {metaParts.length > 0 && (
          <p className="mt-0.5 truncate text-xs font-normal text-muted-foreground">
            {metaParts.join(" · ")}
          </p>
        )}
      </div>
    </div>
  ) : undefined;

  // Refresh icon — rendered next to close button via Modal's extra prop
  const refreshExtra = personId ? (
    <button
      type="button"
      title="从 TMDB 更新"
      className="rounded-md p-1.5 text-[var(--text-muted)] transition-colors hover:bg-black/[0.06] hover:text-[var(--text-secondary)] dark:hover:bg-white/[0.08] cursor-pointer"
      onClick={() => refreshMutation.mutate({ id: personId })}
      disabled={refreshMutation.isPending}
    >
      <RefreshCw
        size={16}
        className={refreshMutation.isPending ? "animate-spin" : ""}
      />
    </button>
  ) : null;

  return (
    <Modal
      open={!!personId}
      onCancel={onClose}
      title={isLoading ? "加载中…" : modalTitle}
      extra={refreshExtra}
      footer={null}
      width={1040}
      centered
      destroyOnClose
      styles={{
        body: {
          maxHeight: "calc(85vh - 56px)",
          overflowY: "auto",
          scrollbarWidth: "thin",
          scrollbarColor: "rgba(128,128,128,0.4) transparent",
        },
      }}
    >
      {isLoading ? (
        <div className="flex h-48 items-center justify-center">
          <Spin />
        </div>
      ) : !person ? (
        <div className="flex h-48 items-center justify-center text-fg-muted">
          未找到该人物
        </div>
      ) : (
        <div>
          {refreshMutation.isPending && (
            <div className="mb-4 flex items-center gap-2 rounded-lg bg-blue-50 px-4 py-3 text-sm text-blue-600 dark:bg-blue-900/20 dark:text-blue-400">
              <RefreshCw size={14} className="animate-spin" />
              正在从 TMDB 获取演员数据…
            </div>
          )}

          {/* ── 顶部：基本信息 ── */}
          <div className="mb-6 flex flex-col gap-2">
            {/* 原名 */}
            {person.originalName && person.originalName !== person.name && (
              <InfoRow label="原名" value={person.originalName} />
            )}
            {/* 性别 */}
            {person.gender && GENDER_LABELS[person.gender] && (
              <InfoRow label="性别" value={GENDER_LABELS[person.gender]} />
            )}
            {/* 已知职位 */}
            {deptLabel && <InfoRow label="职位" value={deptLabel} />}
            {/* 生日 */}
            {person.birthday && (
              <InfoRow label="生日" value={person.birthday} />
            )}
            {/* 忌日 */}
            {person.deathday && (
              <InfoRow label="忌日" value={person.deathday} />
            )}
            {/* 出生地 */}
            {person.birthplace && (
              <InfoRow label="出生地" value={person.birthplace} />
            )}
            {/* 人气 */}
            {person.popularity != null && person.popularity > 0 && (
              <InfoRow label="人气" value={person.popularity.toFixed(1)} />
            )}
            {/* 别名 */}
            {person.aliases && person.aliases.length > 0 && (
              <div className="flex items-start gap-2">
                <span className="mt-0.5 w-14 flex-shrink-0 text-xs font-medium text-fg-muted">
                  别名
                </span>
                <div className="flex flex-wrap gap-1.5">
                  {person.aliases.map((alias) => (
                    <span
                      key={alias}
                      className="rounded bg-fill-tertiary px-1.5 py-0.5 text-xs text-fg-secondary dark:bg-gray-800"
                    >
                      {alias}
                    </span>
                  ))}
                </div>
              </div>
            )}
            {/* 外部链接 */}
            {(person.tmdbId || person.imdbId) && (
              <div className="flex items-center gap-3 pt-1">
                {person.tmdbId && (
                  <a
                    href={`https://www.themoviedb.org/person/${person.tmdbId}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-blue-500 hover:text-blue-400"
                  >
                    TMDB ↗
                  </a>
                )}
                {person.imdbId && (
                  <a
                    href={`https://www.imdb.com/name/${person.imdbId}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-yellow-500 hover:text-yellow-400"
                  >
                    IMDb ↗
                  </a>
                )}
              </div>
            )}
          </div>

          {/* ── 简介 ── */}
          {person.biography && (
            <section className="mb-6">
              <SectionTitle>简介</SectionTitle>
              <div
                className="overflow-hidden transition-[max-height] duration-300 ease-in-out"
                style={{ maxHeight: bioExpanded ? "2000px" : "200px" }}
              >
                <p className="whitespace-pre-line text-sm leading-relaxed text-fg-secondary">
                  {person.biography}
                </p>
              </div>
              {person.biography.length > 300 && (
                <button
                  type="button"
                  className="mt-2 cursor-pointer text-xs text-blue-500 hover:text-blue-400"
                  onClick={() => setBioExpanded((v) => !v)}
                >
                  {bioExpanded ? "收起" : "阅读全部"}
                </button>
              )}
            </section>
          )}

          {/* ── 库内作品 ── */}
          {Object.entries(grouped).map(([role, credits]) => {
            if (!credits?.length) return null;
            const withMedia = credits.filter((c) => c?.mediaTitle);
            if (!withMedia.length) return null;
            return (
              <section key={role} className="mb-6">
                <SectionTitle>
                  {ROLE_LABELS[role] ?? role} 作品
                  <span className="ml-2 text-sm font-normal text-fg-muted">
                    ({withMedia.length})
                  </span>
                </SectionTitle>
                <HorizontalScroll innerClassName="gap-3 px-0.5 pb-2 pt-0.5">
                  {withMedia.map((c) => {
                    const posterSrc = resolveMediaImage(
                      null,
                      c?.mediaPosterPath,
                    );
                    return (
                      <button
                        key={c?.id}
                        type="button"
                        className="group w-[100px] flex-shrink-0 cursor-pointer overflow-hidden rounded-lg bg-surface-base text-left transition-shadow hover:shadow-md dark:bg-gray-800/50"
                        onClick={() => {
                          onClose();
                          if (c?.movieId)
                            navigate(
                              `/movies/${c.movieId}`,
                              c?.mediaTitle ?? "Movie",
                            );
                          else if (c?.tvShowId)
                            navigate(
                              `/tv/${c.tvShowId}`,
                              c?.mediaTitle ?? "TV Show",
                            );
                        }}
                      >
                        <div className="relative aspect-[2/3] overflow-hidden bg-[var(--bg-skeleton)]">
                          {posterSrc ? (
                            <img
                              src={posterSrc}
                              alt={c?.mediaTitle ?? ""}
                              className="h-full w-full object-cover transition-transform group-hover:scale-105"
                              loading="lazy"
                            />
                          ) : (
                            <div className="flex h-full items-center justify-center text-fg-muted">
                              <Film size={32} />
                            </div>
                          )}
                        </div>
                        <div className="p-1.5">
                          <p className="truncate text-xs font-medium text-fg-primary">
                            {c?.mediaTitle}
                          </p>
                          {c?.character && (
                            <p className="truncate text-[11px] text-fg-muted">
                              饰 {c.character}
                            </p>
                          )}
                          {c?.mediaYear && (
                            <p className="text-[11px] text-fg-muted">
                              {c.mediaYear}
                            </p>
                          )}
                        </div>
                      </button>
                    );
                  })}
                </HorizontalScroll>
              </section>
            );
          })}

          {!hasCreditsInLibrary && (
            <p className="py-4 text-center text-sm text-fg-muted">
              暂无库内作品数据
            </p>
          )}
        </div>
      )}
    </Modal>
  );
}

function InfoRow({
  label,
  value,
}: {
  label: string;
  value: string | null | undefined;
}) {
  if (!value) return null;
  return (
    <div className="flex items-start gap-2">
      <span className="mt-0.5 w-14 flex-shrink-0 text-xs font-medium text-fg-muted">
        {label}
      </span>
      <span className="text-xs text-fg-primary">{value}</span>
    </div>
  );
}
