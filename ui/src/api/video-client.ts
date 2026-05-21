/**
 * Low-level fetch wrappers for the video app's API domains.
 *
 * All host-shell + sidecar endpoints return a `{ success, data?, error? }`
 * envelope; this module unwraps it and normalises the bare `/` path so it
 * matches axum's route definitions (which only register `/api/...` and
 * `/api/.../{*rest}`, not the literal `/api/.../`).
 */

async function apiFetch<T>(
  baseUrl: string,
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const normalized = path === "/" ? "" : path;
  const url = `${baseUrl}${normalized}`;
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(init.headers as Record<string, string> | undefined),
  };
  const res = await fetch(url, { ...init, headers });
  if (!res.ok) {
    let message = res.statusText;
    try {
      const body = await res.json();
      if (typeof body === "object" && body !== null) {
        if ("error" in body)
          message = String((body as { error: unknown }).error);
        else if ("message" in body)
          message = String((body as { message: unknown }).message);
      }
    } catch {
      // ignore parse error
    }
    throw new Error(`[${res.status}] ${message}`);
  }
  if (res.status === 204) return undefined as T;
  const payload = (await res.json()) as {
    success: boolean;
    data?: T;
    error?: string;
  };
  if (!payload.success) {
    throw new Error(payload.error ?? `Request failed: ${path}`);
  }
  return payload.data as T;
}

/** Routes under /api/apps/video/ (proxied to video sidecar) */
export function videoFetch<T>(path: string, init?: RequestInit): Promise<T> {
  return apiFetch<T>("/api/apps/video", path, init);
}

/** Routes under /api/vfs/ (host shell VFS API) */
export function vfsFetch<T>(path: string, init?: RequestInit): Promise<T> {
  return apiFetch<T>("/api/vfs", path, init);
}

/** Routes under /api/apps/video/online-media/ (proxied to video sidecar) */
export function videoOnlineMediaFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  return apiFetch<T>("/api/apps/video/online-media", path, init);
}

/** Routes under /api/apps/media-organize/ (host shell media-organize API) */
export function mediaOrganizeFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  return apiFetch<T>("/api/apps/media-organize", path, init);
}

/** Routes under /api/apps/downloads/ (host shell downloads API) */
export function downloadsFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  return apiFetch<T>("/api/apps/downloads", path, init);
}
