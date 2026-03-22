//! Constants for file_scrape handler.

pub const SUBTITLE_EXTENSIONS: &[&str] = &[
    ".srt", ".ass", ".ssa", ".sub", ".idx", ".sup", ".vtt",
];

pub const POSTER_NAMES: &[&str] = &["poster.jpg", "poster.png", "folder.jpg", "cover.jpg"];
pub const POSTER_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];
pub const FANART_NAMES: &[&str] = &["fanart.jpg", "backdrop.jpg", "fanart.png", "backdrop.png"];

pub struct ExtraArtDef {
    pub names: &'static [&'static str],
    pub art_type: &'static str,
}

pub const EXTRA_ART: &[ExtraArtDef] = &[
    ExtraArtDef {
        names: &["banner.jpg", "banner.png"],
        art_type: "banner",
    },
    ExtraArtDef {
        names: &[
            "clearlogo.png",
            "clearlogo.jpg",
            "logo.png",
            "logo.jpg",
            "clearart.png",
        ],
        art_type: "clearlogo",
    },
    ExtraArtDef {
        names: &["landscape.jpg", "landscape.png"],
        art_type: "backdrop",
    },
    ExtraArtDef {
        names: &["thumb.jpg", "thumb.png"],
        art_type: "thumb",
    },
    ExtraArtDef {
        names: &["disc.png", "disc.jpg", "discart.png", "discart.jpg"],
        art_type: "discart",
    },
];

/// Map file extension to MIME type.
pub fn guess_mime(filename: &str) -> Option<String> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "mpg" | "mpeg" => "video/mpeg",
        "3gp" => "video/3gpp",
        "rmvb" | "rm" => "application/vnd.rn-realmedia-vbr",
        "vob" => "video/dvd",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "wma" => "audio/x-ms-wma",
        _ => return None,
    };
    Some(mime.to_string())
}

/// Subtitle extension → format string.
pub fn subtitle_ext_to_format(ext: &str) -> &'static str {
    match ext {
        ".srt" | "srt" => "srt",
        ".ass" | "ass" => "ass",
        ".ssa" | "ssa" => "ssa",
        ".sub" | "sub" => "sub",
        ".vtt" | "vtt" => "vtt",
        ".idx" | "idx" => "idx",
        ".sup" | "sup" => "sup",
        _ => "srt",
    }
}

/// Image extension → MIME type.
pub fn image_mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => "image/jpeg",
    }
}

/// Extract image file extension (defaults to "jpg").
pub fn image_storage_ext(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("jpg")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "webp" | "gif" | "bmp" | "avif" => ext,
        _ => "jpg".to_string(),
    }
}
