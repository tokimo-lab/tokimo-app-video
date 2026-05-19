# shell-shim/

Temporary thin proxies/stubs for host-shell facilities that the video app
still depends on during extraction. The remaining 5 files should be
incrementally removed as the SDK gains the matching capabilities:

- `components.ts` — UI helpers (needs SDK component primitives)
- `apps-settings.ts` — PathSelector etc. (needs SDK settings primitives)
- `apps-finder.ts` — file picker (needs SDK picker)
- `apps-media.ts` — media helpers (needs SDK media primitives)
- `apps-media-organize.ts` — organize flow components

Do NOT add new entries here. Migrate new dependencies to `@tokimo/sdk`
or a video-local module instead.
