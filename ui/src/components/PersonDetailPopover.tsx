import { ScrollArea, Spin } from "@tokimo/ui";
import { ExternalLink, Film } from "lucide-react";
import { PersonPlaceholder } from "../shell-shim/apps-media";
import { api } from "./shell-shim/api";
import { resolveMediaImage } from "../shell-shim/lib";
import { useWindowNav } from "../shell-shim/system";

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

// ─── Popover version (compact, used in cast row) ───

export function PersonDetailPopoverContent({
  personId,
  character,
}: {
  personId: string;
  character?: string | null;
}) {
  const { navigate } = useWindowNav();

  const { data: person, isLoading } = api.video.getPersonDetail.useQuery(
    { id: personId },
    { enabled: !!personId },
  );

  if (isLoading) {
    return (
      <div className="flex h-32 items-center justify-center">
        <Spin />
      </div>
    );
  }

  if (!person) {
    return (
      <div className="flex h-24 items-center justify-center text-xs text-fg-muted">
        未找到该人物
      </div>
    );
  }

  const profileSrc = resolveMediaImage(person.profileKey, person.profilePath);

  const deptLabel = person.knownForDepartment
    ? (DEPT_LABELS[person.knownForDepartment] ?? person.knownForDepartment)
    : null;

  const metaParts: string[] = [];
  if (character) metaParts.push(`饰 ${character}`);
  if (deptLabel) metaParts.push(deptLabel);
  if (person.gender && GENDER_LABELS[person.gender])
    metaParts.push(GENDER_LABELS[person.gender]);
  if (person.birthday) {
    const dates = person.deathday
      ? `${person.birthday} — ${person.deathday}`
      : person.birthday;
    metaParts.push(dates);
  }
  if (person.birthplace) metaParts.push(person.birthplace);

  const grouped = (person.credits ?? []).reduce<
    Record<string, NonNullable<typeof person>["credits"]>
  >((acc, c) => {
    const key = c?.role ?? "other";
    if (!acc[key]) acc[key] = [];
    acc[key]!.push(c);
    return acc;
  }, {});

  return (
    <div className="space-y-3 text-[12px]">
      {/* Header: photo + name + meta */}
      <div className="flex items-start gap-3">
        <div className="h-20 w-14 flex-shrink-0 overflow-hidden rounded-md bg-[var(--bg-skeleton)] shadow ring-1 ring-black/10 dark:ring-white/10">
          {profileSrc ? (
            <img
              src={profileSrc}
              alt={person.name}
              className="h-full w-full object-cover"
            />
          ) : (
            <PersonPlaceholder />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-semibold text-fg-primary">
              {person.name}
            </span>
            {person.originalName && person.originalName !== person.name && (
              <span className="truncate text-[11px] text-fg-muted">
                {person.originalName}
              </span>
            )}
          </div>
          {metaParts.length > 0 && (
            <p className="mt-0.5 text-[11px] text-fg-muted">
              {metaParts.join(" · ")}
            </p>
          )}

          {/* Compact info */}
          <div className="mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-fg-secondary">
            {person.popularity != null && person.popularity > 0 && (
              <span>人气 {person.popularity.toFixed(1)}</span>
            )}
            {person.aliases && person.aliases.length > 0 && (
              <span>
                别名 {person.aliases.slice(0, 2).join("、")}
                {person.aliases.length > 2 ? "…" : ""}
              </span>
            )}
          </div>

          {/* External links + refresh */}
          <div className="mt-1.5 flex items-center gap-2">
            {person.tmdbId && (
              <a
                href={`https://www.themoviedb.org/person/${person.tmdbId}`}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-0.5 text-[11px] text-blue-500 hover:text-blue-400"
              >
                TMDB
                <ExternalLink className="h-2.5 w-2.5" />
              </a>
            )}
            {person.imdbId && (
              <a
                href={`https://www.imdb.com/name/${person.imdbId}`}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-0.5 text-[11px] text-yellow-500 hover:text-yellow-400"
              >
                IMDb
                <ExternalLink className="h-2.5 w-2.5" />
              </a>
            )}
          </div>
        </div>
      </div>

      {/* Biography (truncated) */}
      {person.biography && (
        <p className="line-clamp-4 whitespace-pre-line text-[11px] leading-relaxed text-fg-secondary">
          {person.biography}
        </p>
      )}

      {/* Library credits */}
      {Object.entries(grouped).map(([role, credits]) => {
        if (!credits?.length) return null;
        const withMedia = credits.filter((c) => c?.mediaTitle);
        if (!withMedia.length) return null;
        return (
          <section key={role}>
            <p className="mb-1.5 text-[11px] font-semibold text-fg-primary">
              {ROLE_LABELS[role] ?? role} 作品
              <span className="ml-1 font-normal text-fg-muted">
                ({withMedia.length})
              </span>
            </p>
            <ScrollArea
              direction="horizontal"
              hideScrollbar
              innerClassName="gap-2 pb-1"
            >
              {withMedia.map((c) => {
                const posterSrc = resolveMediaImage(null, c?.mediaPosterPath);
                return (
                  <button
                    key={c?.id}
                    type="button"
                    className="group w-[72px] flex-shrink-0 cursor-pointer overflow-hidden rounded-md bg-surface-elevated text-left transition-shadow hover:shadow-md"
                    onClick={() => {
                      if (c?.videoItemId)
                        navigate(
                          `/movies/${c.videoItemId}`,
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
                          <Film size={20} />
                        </div>
                      )}
                    </div>
                    <div className="p-1">
                      <p className="truncate text-[10px] font-medium text-fg-primary">
                        {c?.mediaTitle}
                      </p>
                      {c?.character && (
                        <p className="truncate text-[9px] text-fg-muted">
                          饰 {c.character}
                        </p>
                      )}
                    </div>
                  </button>
                );
              })}
            </ScrollArea>
          </section>
        );
      })}
    </div>
  );
}
