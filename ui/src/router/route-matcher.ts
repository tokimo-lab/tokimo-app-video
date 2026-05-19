/**
 * Route matcher — matches URL-like paths against route patterns.
 *
 * Patterns use `:param` segments for dynamic values:
 *   matchRoute("/movies/abc-123", ["/", "/movies/:videoItemId"])
 *   → { pattern: "/movies/:videoItemId", params: { videoItemId: "abc-123" } }
 *
 * Copied verbatim from packages/web/src/system/window/route-matcher.ts
 */

export interface RouteMatch {
  pattern: string;
  params: Record<string, string>;
}

/**
 * Match a route path against an ordered list of patterns.
 * Returns the first match with extracted params, or null.
 */
export function matchRoute(
  route: string,
  patterns: string[],
): RouteMatch | null {
  const segments = splitSegments(route);

  for (const pattern of patterns) {
    const patternSegments = splitSegments(pattern);
    if (segments.length !== patternSegments.length) continue;

    const params: Record<string, string> = {};
    let matched = true;

    for (let i = 0; i < patternSegments.length; i++) {
      const ps = patternSegments[i];
      if (ps.startsWith(":")) {
        params[ps.slice(1)] = decodeURIComponent(segments[i]);
      } else if (ps !== segments[i]) {
        matched = false;
        break;
      }
    }

    if (matched) return { pattern, params };
  }

  return null;
}

/** Build a route path from a pattern and params. */
export function buildRoute(
  pattern: string,
  params?: Record<string, string>,
): string {
  if (!params || pattern === "/") return pattern;
  return (
    "/" +
    splitSegments(pattern)
      .map((seg) =>
        seg.startsWith(":")
          ? encodeURIComponent(params[seg.slice(1)] ?? "")
          : seg,
      )
      .join("/")
  );
}

function splitSegments(path: string): string[] {
  return path.split("/").filter(Boolean);
}
