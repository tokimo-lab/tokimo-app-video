export interface VideoPlayerSourceMetadata extends Record<string, unknown> {
  appId: "video";
  videoItemId?: string;
  episodeId?: string;
  tvShowId?: string;
  imdbId?: string | null;
  tmdbId?: string | null;
  watchHistoryId?: string;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function optionalNullableString(value: unknown): string | null | undefined {
  if (value === null) return null;
  return optionalString(value);
}

export function getVideoSourceMetadata(
  sourceMetadata?: Record<string, unknown>,
): VideoPlayerSourceMetadata | null {
  if (!sourceMetadata || sourceMetadata.appId !== "video") return null;
  return {
    appId: "video",
    videoItemId: optionalString(sourceMetadata.videoItemId),
    episodeId: optionalString(sourceMetadata.episodeId),
    tvShowId: optionalString(sourceMetadata.tvShowId),
    imdbId: optionalNullableString(sourceMetadata.imdbId),
    tmdbId: optionalNullableString(sourceMetadata.tmdbId),
    watchHistoryId: optionalString(sourceMetadata.watchHistoryId),
  };
}

export function createVideoSourceMetadata(
  input: Omit<VideoPlayerSourceMetadata, "appId">,
): VideoPlayerSourceMetadata {
  return { appId: "video", ...input };
}
