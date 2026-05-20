// =============================================================================
// ⚠️ CROSS-APP DEPRECATED: photo module inside video sidecar ⚠️
// =============================================================================
// Photo domain code does not belong in the video app. This subtree exists only
// because the video sidecar was extracted from a monolith that shared media
// repositories across photo/music/book/video.
//
// DEADLINE: remove once `tokimo-app-photo` sidecar exists and absorbs these
// repos. Until then, treat this as read-only legacy — DO NOT add features.
// See plan.md F9 (cross-app marker) and architecture migration plan.
// =============================================================================

pub mod repos;
