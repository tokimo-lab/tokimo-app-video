import { resolveMediaImage } from "@tokimo/sdk";
import { ScrollArea, Spin } from "@tokimo/ui";
import { ExternalLink, Film } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useVideoNav } from "../router/useVideoNav";
import { PersonPlaceholder } from "../shell-shim/apps-media";

// --- constants ---

const ROLE_LABELS: Record<string, string> = {
  actor: "media.detail.roles.actor",
  director: "media.detail.roles.director",
  writer: "media.detail.roles.writer",
  producer: "media.detail.roles.producer",
  composer: "media.detail.roles.composer",
  cinematographer: "media.detail.roles.cinematographer",
};

const GENDER_LABELS: Record<string, string> = {
  male: "media.detail.gender.male",
  female: "media.detail.gender.female",
  "non-binary": "media.detail.gender.nonBinary",
  unknown: "media.detail.unknown",
};

const DEPT_LABELS: Record<string, string> = {
  Acting: "media.detail.departments.acting",
  Directing: "media.detail.departments.directing",
  Writing: "media.detail.departments.writing",
  Production: "media.detail.departments.production",
  Sound: "media.detail.departments.sound",
  Camera: "media.detail.departments.camera",
  "Costume & Make-Up": "media.detail.departments.costumeMakeup",
  "Visual Effects": "media.detail.departments.visualEffects",
  Editing: "media.detail.departments.editing",
  Art: "media.detail.departments.art",
  Crew: "media.detail.departments.crew",
};

// ─── Popover version (compact, used in cast row) ───

export function PersonDetailPopoverContent({
  personId,
  character,
}: {
  personId: string;
  character?: string | null;
}) {
  const { navigate } = useVideoNav();
  const { t } = useTranslation();

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
        {t("media.detail.personNotFound")}
      </div>
    );
  }

  const profileSrc = resolveMediaImage(person.profileKey, person.profilePath);

  const deptLabel = person.knownForDepartment
    ? DEPT_LABELS[person.knownForDepartment]
      ? t(DEPT_LABELS[person.knownForDepartment])
      : person.knownForDepartment
    : null;

  const metaParts: string[] = [];
  if (character) metaParts.push(t("media.detail.asCharacter", { character }));
  if (deptLabel) metaParts.push(deptLabel);
  if (person.gender && GENDER_LABELS[person.gender])
    metaParts.push(t(GENDER_LABELS[person.gender]));
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
        <div className="h-20 w-14 flex-shrink-0 overflow-hidden rounded-md bg-[var(--color-fill-skeleton)] shadow ring-1 ring-black/10 dark:ring-white/10">
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
              <span>
                {t("media.detail.popularity", {
                  value: person.popularity.toFixed(1),
                })}
              </span>
            )}
            {person.aliases && person.aliases.length > 0 && (
              <span>
                {t("media.detail.aliases", {
                  aliases: person.aliases.slice(0, 2).join("、"),
                })}
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
              {ROLE_LABELS[role] ? t(ROLE_LABELS[role]) : role}{" "}
              {t("media.detail.works")}
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
                    className="group w-[72px] flex-shrink-0 cursor-pointer overflow-hidden rounded-md bg-surface-raised text-left transition-shadow hover:shadow-md"
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
                    <div className="relative aspect-[2/3] overflow-hidden bg-[var(--color-fill-skeleton)]">
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
                          {t("media.detail.asCharacter", {
                            character: c.character,
                          })}
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
