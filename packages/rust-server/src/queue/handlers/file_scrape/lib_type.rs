/// All supported library types — single source of truth.
/// Parsed once at the `handle()` entry point; never matched on raw strings again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibType {
    Movie,
    Adult,
    Custom,
    OnlineVideo,
    Tv,
    Anime,
    Music,
    Novel,
    Photo,
}

impl LibType {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "movie" => Ok(Self::Movie),
            "adult" => Ok(Self::Adult),
            "custom" => Ok(Self::Custom),
            "online_video" => Ok(Self::OnlineVideo),
            "tv" => Ok(Self::Tv),
            "anime" => Ok(Self::Anime),
            "music" => Ok(Self::Music),
            "novel" => Ok(Self::Novel),
            "photo" => Ok(Self::Photo),
            other => Err(format!("Unknown lib_type: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Adult => "adult",
            Self::Custom => "custom",
            Self::OnlineVideo => "online_video",
            Self::Tv => "tv",
            Self::Anime => "anime",
            Self::Music => "music",
            Self::Novel => "novel",
            Self::Photo => "photo",
        }
    }

    /// True for all library types stored as movies (movie_id, not episode_id).
    pub fn is_movie_family(self) -> bool {
        matches!(self, Self::Movie | Self::Adult | Self::Custom | Self::OnlineVideo)
    }

    /// True for TV / Anime libraries.
    pub fn is_tv_family(self) -> bool {
        matches!(self, Self::Tv | Self::Anime)
    }

    /// True when TMDB should be queried for this library type.
    pub fn uses_tmdb(self) -> bool {
        matches!(self, Self::Movie | Self::Adult | Self::Tv | Self::Anime)
    }

    pub fn is_adult(self) -> bool {
        self == Self::Adult
    }
}
