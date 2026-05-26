import { useQueryClient } from "@tanstack/react-query";
import {
  type AppEntityEvent,
  type ShellJobEvent,
  useAppEntityEvents,
  useJobEvents,
} from "@tokimo/sdk";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { VideoOutput } from "../api/types";

const VIDEO_SCAN_JOB_TYPES = ["tv_scrape", "movie_scrape", "file_scrape"];

export interface VideoLibraryProgressState {
  isActive: boolean;
  pct: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringField(record: Record<string, unknown> | null, key: string) {
  const value = record?.[key];
  return typeof value === "string" && value.length > 0 ? value : null;
}

function numberField(record: Record<string, unknown> | null, key: string) {
  const value = record?.[key];
  return typeof value === "number" ? value : null;
}

function getJobRecord(event: ShellJobEvent): Record<string, unknown> | null {
  if (isRecord(event.job)) return event.job;
  return isRecord(event.data) ? event.data : null;
}

function extractVideoLibraryId(event: ShellJobEvent) {
  const job = getJobRecord(event);
  if (!job) return null;
  const params = isRecord(job.params) ? job.params : null;
  const data = isRecord(job.data) ? job.data : null;
  return (
    stringField(params, "videoId") ??
    stringField(params, "appId") ??
    stringField(data, "videoId") ??
    stringField(data, "appId") ??
    stringField(job, "videoId")
  );
}

function getJobStatus(event: ShellJobEvent) {
  const job = getJobRecord(event);
  return stringField(job, "status");
}

function getJobProgress(event: ShellJobEvent) {
  const job = getJobRecord(event);
  const data = isRecord(job?.data) ? job.data : null;
  const rich = isRecord(data?.progress) ? data.progress : null;
  const current = numberField(rich, "current");
  const total = numberField(rich, "total");
  const progress = numberField(job, "progress") ?? 0;
  const pct =
    current !== null && total !== null && total > 0
      ? Math.round((current / total) * 100)
      : progress;
  return {
    pct: Math.max(0, Math.min(100, pct)),
    label: stringField(rich, "label") ?? "",
  };
}

export function useVideoLibraryProgress(
  categories: VideoOutput[] | undefined,
): Record<string, VideoLibraryProgressState> {
  const queryClient = useQueryClient();
  const [activeLibIds, setActiveLibIds] = useState<Set<string>>(new Set());
  const [progressPct, setProgressPct] = useState<Record<string, number>>({});
  const categoryIdsRef = useRef<Set<string>>(new Set());
  const pendingByLibRef = useRef(new Map<string, Set<string>>());

  useEffect(() => {
    categoryIdsRef.current = new Set(
      (categories ?? []).map((category) => category.id),
    );
  }, [categories]);

  const refreshContent = useCallback(() => {
    api.video.listVideoItems.invalidate(queryClient);
    api.video.listTvShows.invalidate(queryClient);
    api.video.getRecentlyAdded.invalidate(queryClient);
    api.video.listGenres.invalidate(queryClient);
    api.video.listCountries.invalidate(queryClient);
    api.video.list.invalidate(queryClient);
  }, [queryClient]);

  const handleJobEvent = useCallback(
    (event: ShellJobEvent) => {
      if (event.type !== "job_update") return;
      const libraryId = extractVideoLibraryId(event);
      if (!libraryId || !categoryIdsRef.current.has(libraryId)) return;

      const job = getJobRecord(event);
      const jobId = stringField(job, "id");
      const status = getJobStatus(event);
      const { pct } = getJobProgress(event);
      if (
        status === "completed" ||
        status === "failed" ||
        status === "cancelled"
      ) {
        if (!jobId) {
          refreshContent();
        } else {
          const pendingJobs = pendingByLibRef.current.get(libraryId);
          if (pendingJobs) {
            const wasNonEmpty = pendingJobs.size > 0;
            pendingJobs.delete(jobId);
            if (wasNonEmpty && pendingJobs.size === 0) {
              refreshContent();
              pendingByLibRef.current.delete(libraryId);
            }
          } else {
            refreshContent();
          }
        }
        setProgressPct((prev) => {
          const next = { ...prev };
          if (status === "completed") {
            next[libraryId] = 100;
          } else {
            delete next[libraryId];
          }
          return next;
        });
        setActiveLibIds((prev) => {
          const next = new Set(prev);
          next.delete(libraryId);
          return next.size === prev.size ? prev : next;
        });
        return;
      }

      if (status === "pending" || status === "running") {
        if (jobId) {
          let pendingJobs = pendingByLibRef.current.get(libraryId);
          if (!pendingJobs) {
            pendingJobs = new Set();
            pendingByLibRef.current.set(libraryId, pendingJobs);
          }
          pendingJobs.add(jobId);
        }
      }

      setProgressPct((prev) => ({ ...prev, [libraryId]: pct }));
      setActiveLibIds((prev) => {
        if (prev.has(libraryId)) return prev;
        const next = new Set(prev);
        next.add(libraryId);
        return next;
      });
    },
    [refreshContent],
  );

  const handleEntityEvent = useCallback(
    (event: AppEntityEvent) => {
      const scope = event.scope ?? "";
      const libraryId = scope.startsWith("library:")
        ? scope.slice("library:".length)
        : null;
      if (!libraryId || !categoryIdsRef.current.has(libraryId)) return;
      refreshContent();
    },
    [refreshContent],
  );

  useJobEvents({
    jobTypes: VIDEO_SCAN_JOB_TYPES,
    onEvent: handleJobEvent,
    enabled: (categories ?? []).length > 0,
  });

  useAppEntityEvents({
    appId: "video",
    kind: "video_item",
    onEvent: handleEntityEvent,
  });

  useEffect(() => {
    if (!categories) return;
    const syncing = categories
      .filter((category) => category.syncStatus === "syncing")
      .map((category) => category.id);
    if (syncing.length === 0) return;
    setActiveLibIds((prev) => {
      const next = new Set(prev);
      for (const id of syncing) next.add(id);
      return next.size !== prev.size ? next : prev;
    });
  }, [categories]);

  const syncProgress: Record<string, VideoLibraryProgressState> = {};
  for (const id of activeLibIds) {
    syncProgress[id] = { isActive: true, pct: progressPct[id] ?? 0 };
  }
  return syncProgress;
}
