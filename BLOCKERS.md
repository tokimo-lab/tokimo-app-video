# tokimo-app-video — Known Blockers

This file documents deliberate `unimplemented!()` markers and wiring gaps in the
video app submodule, together with the reason each is blocked and the work needed
to unblock it.

---

## B-1  Photo branch in `file_scrape` (P0 blocker, `unimplemented!`)

**File:** `src/queue/handlers/file_scrape.rs`  
**Trigger:** `LibType::Photo` dispatch arm inside `handle()`  
**Code:**
```rust
// blocker: photo scrape module does not exist in the video submodule.
// This belongs to tokimo-app-photo. To unblock, either:
//   (a) move shared media scraping into a common crate both apps depend on, or
//   (b) remove LibType::Photo from the video app's schema entirely.
unimplemented!("blocker: photo scrape not implemented in video app");
```
**What's missing:** `crate::services::media::scrape::photo` — that module lives in
the photo app and was not copied here.  
**To unblock:** Extract the shared file-scrape pipeline into a new
`tokimo-scrape-common` crate (or similar) that both apps can depend on, then wire
`LibType::Photo` to the correct handler.

---

## B-2  `bus_notify_job` not wired at service-layer `create_job` sites

**Files affected (all `image_upload` jobs):**
- `src/services/scrape/tv.rs` — `upsert_season` (line ≈613), `upsert_episode` (line ≈704)
- `src/services/scrape/shared/artwork.rs` — `dispatch_tmdb_image_job` (line ≈52)
- `src/services/common.rs` — `create_person_scrape_job` (line ≈1469)
- `src/queue/tmdb_person_scrape.rs` — image_upload dispatch (line ≈85)

**Why not wired:** These functions accept only `&DatabaseConnection`; no
`AppState`/`AppCtx` is in scope. Threading the bus client down would require
adding an optional `Option<&Arc<OnceLock<Arc<BusClient>>>>` parameter to ~8
functions.  
**Impact:** Low — these are all fire-and-forget `image_upload` background jobs.
The bus client in the current implementation is used only to update the job queue
UI; missing notification here means the task panel may lag by one poll interval.  
**To unblock:** Either (a) pass `&AppCtx` as a shared context struct through the
service layer, or (b) implement an in-process event bus that services can emit
on without holding `AppState`.

---

## B-3  `JobOutput` user_id placeholder

**File:** `src/state.rs` — `bus_notify_job()` method  
**Issue:** `JobOutput` in this submodule does not carry a `user_id` field (unlike
the main server's `JobOutput`). The bus protocol `UpsertJobPayload` requires a
`user_id: Uuid`. The current code uses `Uuid::nil()` as a placeholder.  
**Impact:** The bus consumer may associate enqueued jobs with the nil user instead
of the actual triggering user.  
**To unblock:** Either add `user_id` to `JobOutput` (and populate it from
`jobs::Model`) or pass the acting user's id as an explicit parameter to
`bus_notify_job`.

---

## B-4  `cancel_jobs_by_app_id` does not emit bus events per job

**Files:** `src/handlers/crud.rs` (line ≈148), `src/services/app_sync.rs`  
**Why not wired:** The repo function cancels in bulk (returns a `DeleteResult`
with just a row count, not individual job records). Emitting per-job events would
require a `SELECT` before the bulk `UPDATE` to collect job ids.  
**Impact:** Low — cancel is an admin operation; the task panel can re-query.  
**To unblock:** Change `cancel_jobs_by_app_id` to return the cancelled job ids,
then iterate and call `bus_notify_job` for each.
