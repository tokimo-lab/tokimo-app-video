pub mod file_repo;
pub mod media_content_repo;
pub mod music_repo; // Stubbed out - entities removed in B4
pub mod playback_repo;
pub mod playback_session_repo;
pub mod video_repo;

pub use file_repo::VideoFileRepo;
pub use media_content_repo::MediaContentRepo;
// MusicRepo removed - entities no longer available
pub use playback_repo::PlaybackRepo;
pub use playback_session_repo::{CreatePlaybackSessionInput, PlaybackSessionRepo};
pub use video_repo::VideoRepo;
pub mod vfs_repo;
