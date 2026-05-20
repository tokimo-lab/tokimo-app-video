/**
 * Typed React Query hooks for the video app's REST endpoints.
 *
 * Each export follows the same shape:
 *   - `queryKey(input?)` — array used by React Query
 *   - `useQuery(input?, opts?)` — React Query hook
 *   - `useMutation(opts?)` — React Query mutation hook
 *   - `invalidate(qc, input?)` — invalidate (optionally scoped to input)
 *   - `setData(qc, input, updater)` — optimistic cache update
 *   - `fetch(input?)` — bypass React Query (used by polling helpers)
 */

import { type QueryClient, useMutation, useQuery } from "@tanstack/react-query";
import type {
  AnalyzeOnlineMediaRequest,
  AnalyzeOnlineMediaResponse,
  FileProbeResult,
  LinkMode,
  OrganizeItem,
  OrganizeSession,
  PersonDetailOutput,
  StartOnlineMediaDownloadInput,
  StartOnlineMediaDownloadOutput,
  TvShowDetailOutput,
  TvShowOutput,
  VfsDto,
  VideoItemDetailOutput,
  VideoItemOutput,
  VideoOutput,
  VideoSyncProgressOutput,
  WatchHistoryEntry,
} from "./types";
import {
  mediaOrganizeFetch,
  onlineMediaFetch,
  vfsFetch,
  videoFetch,
} from "./video-client";

// ── Common input shapes ────────────────────────────────────────────────────

export interface ListMediaInput {
  id: string;
  page?: number;
  pageSize?: number;
  sortBy?: string;
  sortDir?: string;
  genreId?: string;
  search?: string;
  country?: string;
  favorite?: boolean;
  resolution?: string;
  runtime?: string;
}

function buildMediaListParams(input: ListMediaInput): string {
  const params = new URLSearchParams();
  if (input.page != null) params.set("page", String(input.page));
  if (input.pageSize != null) params.set("pageSize", String(input.pageSize));
  if (input.sortBy != null) params.set("sortBy", input.sortBy);
  if (input.sortDir != null) params.set("sortDir", input.sortDir);
  if (input.genreId != null) params.set("genreId", input.genreId);
  if (input.search != null) params.set("search", input.search);
  if (input.country != null) params.set("country", input.country);
  if (input.favorite != null) params.set("favorite", String(input.favorite));
  if (input.resolution != null) params.set("resolution", input.resolution);
  if (input.runtime != null) params.set("runtime", input.runtime);
  const qs = params.toString();
  return qs ? `?${qs}` : "";
}

// ─── Video library API ─────────────────────────────────────────────────────

const VIDEO_KEY = "video";

export const apiVideoList = {
  queryKey: (): unknown[] => [VIDEO_KEY, "list"],
  useQuery: (opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoList.queryKey(),
      queryFn: () => videoFetch<VideoOutput[]>("/"),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: apiVideoList.queryKey() }),
};

export interface CreateVideoInput {
  name: string;
  type: string;
  description?: string | null;
  posterPath?: string | null;
  scrapeAgents?: string[] | null;
  settings?: unknown;
  sources?: Array<{
    sourceId: string;
    rootPath: string;
    isDefaultDownload?: boolean;
  }>;
  [key: string]: unknown;
}

export const apiVideoCreate = {
  useMutation: (opts?: {
    onSuccess?: (data: VideoOutput) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: CreateVideoInput) =>
        videoFetch<VideoOutput>("/", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export interface UpdateVideoInput {
  id: string;
  name?: string;
  type?: string;
  description?: string | null;
  posterPath?: string | null;
  scrapeAgents?: string[] | null;
  settings?: unknown;
  sources?: Array<{
    sourceId: string;
    rootPath: string;
    isDefaultDownload?: boolean;
  }>;
  [key: string]: unknown;
}

export const apiVideoUpdate = {
  useMutation: (opts?: {
    onSuccess?: (data: VideoOutput) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: ({ id, ...body }: UpdateVideoInput) =>
        videoFetch<VideoOutput>(`/${encodeURIComponent(id)}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiVideoDelete = {
  useMutation: (opts?: {
    onSuccess?: () => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { id: string }) =>
        videoFetch<void>(`/${encodeURIComponent(input.id)}`, {
          method: "DELETE",
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiVideoSync = {
  useMutation: (opts?: {
    onSuccess?: (data: unknown) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { id: string; clearData?: boolean }) =>
        videoFetch<unknown>(`/${encodeURIComponent(input.id)}/sync`, {
          method: "POST",
          body: JSON.stringify({ clearData: input.clearData ?? false }),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiVideoGetSyncProgress = {
  queryKey: (input: { id: string }): unknown[] => [
    VIDEO_KEY,
    "sync-progress",
    input.id,
  ],
  fetch: (input: { id: string }) =>
    videoFetch<VideoSyncProgressOutput>(
      `/${encodeURIComponent(input.id)}/sync-progress`,
    ),
};

export const apiVideoListVideoItems = {
  queryKey: (input: ListMediaInput): unknown[] => [VIDEO_KEY, "items", input],
  useQuery: (input: ListMediaInput, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoListVideoItems.queryKey(input),
      queryFn: () =>
        videoFetch<VideoItemOutput[]>(
          `/${encodeURIComponent(input.id)}/items${buildMediaListParams(input)}`,
        ),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [VIDEO_KEY, "items"] }),
};

export const apiVideoListTvShows = {
  queryKey: (input: ListMediaInput): unknown[] => [
    VIDEO_KEY,
    "tv-shows",
    input,
  ],
  useQuery: (input: ListMediaInput, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoListTvShows.queryKey(input),
      queryFn: () =>
        videoFetch<TvShowOutput[]>(
          `/${encodeURIComponent(input.id)}/tv-shows${buildMediaListParams(input)}`,
        ),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [VIDEO_KEY, "tv-shows"] }),
};

export const apiVideoListGenres = {
  queryKey: (input: { id: string }): unknown[] => [
    VIDEO_KEY,
    "genres",
    input.id,
  ],
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoListGenres.queryKey(input),
      queryFn: () =>
        videoFetch<Array<{ id: string; tmdbGenreId: number; name: string }>>(
          `/${encodeURIComponent(input.id)}/genres`,
        ),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [VIDEO_KEY, "genres"] }),
};

export const apiVideoListCountries = {
  queryKey: (input: { id: string }): unknown[] => [
    VIDEO_KEY,
    "countries",
    input.id,
  ],
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoListCountries.queryKey(input),
      queryFn: () =>
        videoFetch<string[]>(`/${encodeURIComponent(input.id)}/countries`),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [VIDEO_KEY, "countries"] }),
};

export const apiVideoGetRecentlyAdded = {
  queryKey: (input: { id: string }): unknown[] => [
    VIDEO_KEY,
    "recently-added",
    input.id,
  ],
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoGetRecentlyAdded.queryKey(input),
      queryFn: () =>
        videoFetch<unknown>(`/${encodeURIComponent(input.id)}/recently-added`),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [VIDEO_KEY, "recently-added"] }),
};

export const apiVideoToggleFavorite = {
  useMutation: (opts?: {
    onSuccess?: (data: { isFavorite: boolean }) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { type: "movie" | "tvshow"; id: string }) =>
        videoFetch<{ isFavorite: boolean }>("/toggle-favorite", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiVideoGetVideoItemDetail = {
  queryKey: (input: { id: string }): unknown[] => [VIDEO_KEY, "item", input.id],
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoGetVideoItemDetail.queryKey(input),
      queryFn: () =>
        videoFetch<VideoItemDetailOutput>(
          `/item/${encodeURIComponent(input.id)}`,
        ),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient, input?: { id: string }) => {
    if (input) {
      return qc.invalidateQueries({
        queryKey: apiVideoGetVideoItemDetail.queryKey(input),
      });
    }
    return qc.invalidateQueries({ queryKey: [VIDEO_KEY, "item"] });
  },
};

export const apiVideoGetTvShowDetail = {
  queryKey: (input: { id: string }): unknown[] => [VIDEO_KEY, "tv", input.id],
  fetch: (input: { id: string }) =>
    videoFetch<TvShowDetailOutput>(`/tv/${encodeURIComponent(input.id)}`),
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoGetTvShowDetail.queryKey(input),
      queryFn: () => apiVideoGetTvShowDetail.fetch(input),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient, input?: { id: string }) => {
    if (input) {
      return qc.invalidateQueries({
        queryKey: apiVideoGetTvShowDetail.queryKey(input),
      });
    }
    return qc.invalidateQueries({ queryKey: [VIDEO_KEY, "tv"] });
  },
};

export const apiVideoGetPersonDetail = {
  queryKey: (input: { id: string }): unknown[] => [
    VIDEO_KEY,
    "person",
    input.id,
  ],
  useQuery: (input: { id: string }, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVideoGetPersonDetail.queryKey(input),
      queryFn: () =>
        videoFetch<PersonDetailOutput>(
          `/person/${encodeURIComponent(input.id)}`,
        ),
      enabled: opts?.enabled,
    }),
};

// ─── VFS API ───────────────────────────────────────────────────────────────

const VFS_KEY = "vfs";

export const apiVfsList = {
  queryKey: (): unknown[] => [VFS_KEY, "list"],
  useQuery: (opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiVfsList.queryKey(),
      queryFn: () => vfsFetch<VfsDto[]>("/"),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: apiVfsList.queryKey() }),
};

export const apiVfsProbe = {
  queryKey: (input: { fileSystemId: string; path: string }): unknown[] => [
    VFS_KEY,
    "probe",
    input.fileSystemId,
    input.path,
  ],
  useQuery: (
    input: { fileSystemId: string; path: string },
    opts?: { enabled?: boolean },
  ) =>
    useQuery({
      queryKey: apiVfsProbe.queryKey(input),
      queryFn: () =>
        vfsFetch<FileProbeResult>(
          `/${encodeURIComponent(input.fileSystemId)}/probe?path=${encodeURIComponent(input.path)}`,
        ),
      enabled: opts?.enabled,
    }),
};

// ─── Online media API ──────────────────────────────────────────────────────

export const apiOnlineMediaAnalyze = {
  useMutation: (opts?: {
    onSuccess?: (data: AnalyzeOnlineMediaResponse) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: AnalyzeOnlineMediaRequest) =>
        onlineMediaFetch<AnalyzeOnlineMediaResponse>("/analyze", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiOnlineMediaStartDownload = {
  useMutation: (opts?: {
    onSuccess?: (data: StartOnlineMediaDownloadOutput) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: StartOnlineMediaDownloadInput) =>
        onlineMediaFetch<StartOnlineMediaDownloadOutput>("/start-download", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

// ─── Media Organize API ────────────────────────────────────────────────────

const MO_KEY = "media-organize";

export const apiMediaOrganizeGetSession = {
  queryKey: (): unknown[] => [MO_KEY, "session"],
  useQuery: (opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiMediaOrganizeGetSession.queryKey(),
      queryFn: () => mediaOrganizeFetch<OrganizeSession | null>("/session"),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: apiMediaOrganizeGetSession.queryKey() }),
  setData: (
    qc: QueryClient,
    _input: undefined,
    updater: (
      old: OrganizeSession | null | undefined,
    ) => OrganizeSession | null | undefined,
  ) =>
    qc.setQueryData<OrganizeSession | null>(
      apiMediaOrganizeGetSession.queryKey(),
      updater,
    ),
};

export const apiMediaOrganizeScan = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeSession) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { path: string; sourceId?: string }) =>
        mediaOrganizeFetch<OrganizeSession>("/scan", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeIdentifyItem = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { itemId: string }) =>
        mediaOrganizeFetch<OrganizeItem>(
          `/identify/${encodeURIComponent(input.itemId)}`,
          { method: "POST" },
        ),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeIdentifyAll = {
  useMutation: (opts?: {
    onSuccess?: (data: { started: boolean }) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: () =>
        mediaOrganizeFetch<{ started: boolean }>("/identify-all", {
          method: "POST",
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeSelectMatch = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: {
        itemId: string;
        tmdbId: number;
        mediaType: string;
      }) =>
        mediaOrganizeFetch<OrganizeItem>("/select-match", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeSelectAdultMatch = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { itemId: string; videoId: string }) =>
        mediaOrganizeFetch<OrganizeItem>("/select-adult-match", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeSelectMusicMatch = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { itemId: string; mbReleaseId: string }) =>
        mediaOrganizeFetch<OrganizeItem>("/select-music-match", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeResetMatch = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: { itemId: string }) =>
        mediaOrganizeFetch<OrganizeItem>("/reset-match", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeUpdateTarget = {
  useMutation: (opts?: {
    onSuccess?: (data: OrganizeItem) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input: {
        itemId: string;
        folderId?: string;
        linkMode?: LinkMode;
      }) =>
        mediaOrganizeFetch<OrganizeItem>("/update-target", {
          method: "POST",
          body: JSON.stringify(input),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeExecute = {
  useMutation: (opts?: {
    onSuccess?: (data: unknown) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (input?: { itemIds?: string[] }) =>
        mediaOrganizeFetch<unknown>("/execute", {
          method: "POST",
          body: JSON.stringify(input ?? {}),
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeCancel = {
  useMutation: (opts?: {
    onSuccess?: (data: unknown) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: () =>
        mediaOrganizeFetch<unknown>("/cancel", { method: "POST" }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiMediaOrganizeClear = {
  useMutation: (opts?: {
    onSuccess?: (data: unknown) => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: () =>
        mediaOrganizeFetch<unknown>("/clear", { method: "POST" }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

// ─── Playback API ──────────────────────────────────────────────────────────

const PLAYBACK_KEY = "playback";

export interface WatchHistoryInput {
  videoItemId?: string;
  episodeId?: string;
  tvShowId?: string;
  limit?: number;
}

export const apiPlaybackWatchHistory = {
  queryKey: (input: WatchHistoryInput): unknown[] => [
    PLAYBACK_KEY,
    "watch-history",
    input,
  ],
  fetch: (input: WatchHistoryInput) => {
    const params = new URLSearchParams();
    if (input.videoItemId) params.set("videoItemId", input.videoItemId);
    if (input.episodeId) params.set("episodeId", input.episodeId);
    if (input.tvShowId) params.set("tvShowId", input.tvShowId);
    if (input.limit != null) params.set("limit", String(input.limit));
    const qs = params.toString();
    return videoFetch<WatchHistoryEntry[]>(
      `/playback/watch-history${qs ? `?${qs}` : ""}`,
    );
  },
  useQuery: (input: WatchHistoryInput, opts?: { enabled?: boolean }) =>
    useQuery({
      queryKey: apiPlaybackWatchHistory.queryKey(input),
      queryFn: () => apiPlaybackWatchHistory.fetch(input),
      enabled: opts?.enabled,
    }),
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: [PLAYBACK_KEY, "watch-history"] }),
};

export const apiPlaybackDeleteWatchHistory = {
  useMutation: (opts?: {
    onSuccess?: () => void;
    onError?: (error: Error) => void;
  }) =>
    useMutation({
      mutationFn: (id: string) =>
        videoFetch<void>(`/playback/watch-history/${encodeURIComponent(id)}`, {
          method: "DELETE",
        }),
      onSuccess: opts?.onSuccess,
      onError: opts?.onError,
    }),
};

export const apiPlaybackReportProgress = {
  mutate: (input: {
    watchHistoryId: string;
    position: number;
    duration: number;
  }) =>
    videoFetch<void>("/playback/progress", {
      method: "POST",
      body: JSON.stringify(input),
    }),
};

// ─── Download Manage (only invalidate is used) ─────────────────────────────

export const apiDownloadManageList = {
  queryKey: (): unknown[] => ["downloads", "list"],
  invalidate: (qc: QueryClient) =>
    qc.invalidateQueries({ queryKey: apiDownloadManageList.queryKey() }),
};
