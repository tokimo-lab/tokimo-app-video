import { useQueryClient } from "@tanstack/react-query";
import { type ShellJobEvent, useJobEvents } from "@tokimo/sdk";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { VideoOutput } from "../api/types";

const VIDEO_SCAN_JOB_TYPES = ["tv_scrape", "movie_scrape"];

export interface VideoLibraryProgressState {
  isActive: boolean;
  pct: number;
}

interface SyncSnapshot {
  completed: number;
  running: number;
  pending: number;
  failed: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringField(record: Record<string, unknown> | null, key: string) {
  const value = record?.[key];
  return typeof value === "string" && value.length > 0 ? value : null;
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

export function useVideoLibraryProgress(
  categories: VideoOutput[] | undefined,
): Record<string, VideoLibraryProgressState> {
  const queryClient = useQueryClient();
  const [activeLibIds, setActiveLibIds] = useState<Set<string>>(new Set());
  const [progressPct, setProgressPct] = useState<Record<string, number>>({});
  const lastSyncSnapshotRef = useRef<Record<string, SyncSnapshot>>({});
  const categoryIdsRef = useRef<Set<string>>(new Set());

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

  const throttleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const throttlePendingRef = useRef(false);
  const throttledRefresh = useCallback(() => {
    if (throttleTimerRef.current) {
      throttlePendingRef.current = true;
      return;
    }
    refreshContent();
    throttleTimerRef.current = setTimeout(() => {
      throttleTimerRef.current = null;
      if (throttlePendingRef.current) {
        throttlePendingRef.current = false;
        refreshContent();
      }
    }, 500);
  }, [refreshContent]);

  const handleJobEvent = useCallback(
    (event: ShellJobEvent) => {
      if (event.type !== "job_update") return;
      const libraryId = extractVideoLibraryId(event);
      if (!libraryId || !categoryIdsRef.current.has(libraryId)) return;

      const status = getJobStatus(event);
      if (
        status === "completed" ||
        status === "failed" ||
        status === "cancelled"
      ) {
        throttledRefresh();
        setActiveLibIds((prev) => {
          const next = new Set(prev);
          next.delete(libraryId);
          return next.size === prev.size ? prev : next;
        });
        return;
      }

      setActiveLibIds((prev) => {
        if (prev.has(libraryId)) return prev;
        const next = new Set(prev);
        next.add(libraryId);
        return next;
      });
    },
    [throttledRefresh],
  );

  useJobEvents({
    jobTypes: VIDEO_SCAN_JOB_TYPES,
    onEvent: handleJobEvent,
    enabled: (categories ?? []).length > 0,
  });

  useEffect(() => {
    if (activeLibIds.size === 0) {
      setProgressPct({});
      return;
    }

    const interval = setInterval(async () => {
      const ids = Array.from(activeLibIds);
      const pcts: Record<string, number> = {};
      const terminalIds: string[] = [];
      let shouldRefresh = false;

      await Promise.all(
        ids.map(async (id) => {
          try {
            const data = await queryClient.fetchQuery({
              queryKey: api.video.getSyncProgress.queryKey({ id }),
              queryFn: () => api.video.getSyncProgress.fetch({ id }),
              staleTime: 1000,
            });
            const total =
              data.completed + data.running + data.pending + data.failed;
            pcts[id] =
              total > 0 ? Math.round((data.completed / total) * 100) : 0;

            const prev = lastSyncSnapshotRef.current[id];
            const next = {
              completed: data.completed,
              running: data.running,
              pending: data.pending,
              failed: data.failed,
            };
            lastSyncSnapshotRef.current[id] = next;

            const changed =
              !prev ||
              prev.completed !== next.completed ||
              prev.running !== next.running ||
              prev.pending !== next.pending ||
              prev.failed !== next.failed;
            const inProgress = next.running > 0 || next.pending > 0;

            if (changed || inProgress) shouldRefresh = true;
            if (!inProgress) terminalIds.push(id);
          } catch (err) {
            console.warn("[video] failed to fetch sync progress", err);
            pcts[id] = 0;
          }
        }),
      );

      setProgressPct(pcts);
      if (terminalIds.length > 0) {
        setActiveLibIds((prev) => {
          let mutated = false;
          const next = new Set(prev);
          for (const id of terminalIds) {
            if (next.delete(id)) {
              mutated = true;
              delete lastSyncSnapshotRef.current[id];
            }
          }
          return mutated ? next : prev;
        });
      }
      if (shouldRefresh) throttledRefresh();
    }, 5000);

    return () => clearInterval(interval);
  }, [activeLibIds, queryClient, throttledRefresh]);

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

  useEffect(() => {
    return () => {
      if (throttleTimerRef.current) clearTimeout(throttleTimerRef.current);
    };
  }, []);

  const syncProgress: Record<string, VideoLibraryProgressState> = {};
  for (const id of activeLibIds) {
    syncProgress[id] = { isActive: true, pct: progressPct[id] ?? 0 };
  }
  return syncProgress;
}
