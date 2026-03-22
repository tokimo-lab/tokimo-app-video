//! Filename and season/episode parsing for media files.

/// Extract title and year from a media filename.
pub fn parse_media_filename(filename: &str) -> (String, Option<i32>) {
    let name = filename.rsplit_once('.').map(|(n, _)| n).unwrap_or(filename);

    if let Some(result) = extract_year_in_brackets(name) {
        return result;
    }
    if let Some(result) = extract_year_with_dots(name) {
        return result;
    }
    if let Some(result) = extract_year_with_spaces(name) {
        return result;
    }

    let clean = name.replace('.', " ").replace('_', " ");
    (clean.trim().to_string(), None)
}

fn extract_year_in_brackets(name: &str) -> Option<(String, Option<i32>)> {
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some(pos) = name.rfind(open) {
            let after = &name[pos + 1..];
            if let Some(end) = after.find(close) {
                if let Some(year) = parse_year_str(&after[..end]) {
                    let title = name[..pos].trim();
                    if !title.is_empty() {
                        return Some((title.to_string(), Some(year)));
                    }
                }
            }
        }
    }
    None
}

fn extract_year_with_dots(name: &str) -> Option<(String, Option<i32>)> {
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if let Some(year) = parse_year_str(part) {
            let title = parts[..i].join(" ").trim().to_string();
            if !title.is_empty() {
                return Some((title, Some(year)));
            }
        }
    }
    None
}

fn extract_year_with_spaces(name: &str) -> Option<(String, Option<i32>)> {
    let parts: Vec<&str> = name.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if let Some(year) = parse_year_str(part) {
            let title = parts[..i].join(" ").trim().to_string();
            if !title.is_empty() {
                return Some((title, Some(year)));
            }
        }
    }
    None
}

fn parse_year_str(s: &str) -> Option<i32> {
    if s.len() == 4 {
        if let Ok(y) = s.parse::<i32>() {
            if (1900..=2099).contains(&y) {
                return Some(y);
            }
        }
    }
    None
}

/// Extract season and episode numbers from a filename.
/// Handles: S01E02, s1e2, 1x02, etc.
pub fn parse_season_episode(filename: &str) -> Option<(i32, i32)> {
    let lower = filename.to_ascii_lowercase();
    let bytes = lower.as_bytes();

    // "s01e02" pattern
    for (i, &b) in bytes.iter().enumerate() {
        if b != b's' { continue; }
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() { continue; }
        let after_s = &lower[i + 1..];
        if let Some((season_str, rest)) = split_at_non_digit(after_s) {
            if let Ok(season) = season_str.parse::<i32>() {
                if rest.starts_with('e') {
                    if let Some((ep_str, _)) = split_at_non_digit(&rest[1..]) {
                        if let Ok(ep) = ep_str.parse::<i32>() {
                            return Some((season, ep));
                        }
                    }
                }
            }
        }
    }

    // "1x02" pattern
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'x' || i == 0 { continue; }
        let before_x = &lower[..i];
        let season_start = before_x
            .rfind(|c: char| !c.is_ascii_digit())
            .map(|p| p + 1)
            .unwrap_or(0);
        let season_str = &before_x[season_start..];
        let after_x = &lower[i + 1..];
        if let Some((ep_str, _)) = split_at_non_digit(after_x) {
            if let (Ok(season), Ok(ep)) = (season_str.parse::<i32>(), ep_str.parse::<i32>()) {
                if season > 0 && ep > 0 {
                    return Some((season, ep));
                }
            }
        }
    }

    None
}

fn split_at_non_digit(s: &str) -> Option<(&str, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 { return None; }
    Some((&s[..end], &s[end..]))
}

/// Check whether the filename looks like a Blu-ray disc placeholder.
pub fn is_placeholder_disc_stem(filename: &str, parsed_title: &str) -> bool {
    let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "m2ts" | "mts" | "vob") {
        return false;
    }
    let t = parsed_title.trim();
    t.len() >= 4 && t.chars().all(|c| c.is_ascii_digit())
}

/// Detect subtitle language from filename.
pub fn detect_subtitle_language(filename: &str) -> String {
    let without_ext = filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename);
    let parts: Vec<&str> = without_ext.split('.').collect();
    if let Some(&last) = parts.last() {
        if last != without_ext && last.len() >= 2 && last.len() <= 16 {
            let is_lang = last.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if is_lang && !last.chars().all(|c| c.is_ascii_digit()) {
                return last.to_string();
            }
        }
    }
    "und".to_string()
}

/// Find a poster file named `{stem}.{ext}` among directory entries.
pub fn find_stem_poster_filename(dir_entries: &[String], stem: &str) -> Option<String> {
    let lower_stem = stem.to_ascii_lowercase();
    for ext in super::constants::POSTER_EXTENSIONS {
        let candidate = format!("{lower_stem}.{ext}");
        if let Some(entry) = dir_entries.iter().find(|e| e.to_ascii_lowercase() == candidate) {
            return Some(entry.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename_year_in_parens() {
        let (title, year) = parse_media_filename("The Matrix (1999).mkv");
        assert_eq!(title, "The Matrix");
        assert_eq!(year, Some(1999));
    }

    #[test]
    fn test_parse_filename_year_in_brackets() {
        let (title, year) = parse_media_filename("Movie [2024].mp4");
        assert_eq!(title, "Movie");
        assert_eq!(year, Some(2024));
    }

    #[test]
    fn test_parse_filename_year_with_dots() {
        let (title, year) = parse_media_filename("movie.2024.1080p.BluRay.mkv");
        assert_eq!(title, "movie");
        assert_eq!(year, Some(2024));
    }

    #[test]
    fn test_parse_filename_no_year() {
        let (title, year) = parse_media_filename("some_movie.mkv");
        assert_eq!(title, "some movie");
        assert_eq!(year, None);
    }

    #[test]
    fn test_parse_season_episode_standard() {
        assert_eq!(parse_season_episode("show.S01E02.720p.mkv"), Some((1, 2)));
        assert_eq!(parse_season_episode("show.s1e3.mkv"), Some((1, 3)));
    }

    #[test]
    fn test_parse_season_episode_x_format() {
        assert_eq!(parse_season_episode("show.1x02.mkv"), Some((1, 2)));
    }

    #[test]
    fn test_parse_season_episode_none() {
        assert_eq!(parse_season_episode("movie.2024.mkv"), None);
    }

    #[test]
    fn test_detect_subtitle_language() {
        assert_eq!(detect_subtitle_language("Movie.en.srt"), "en");
        assert_eq!(detect_subtitle_language("Movie.zh-Hans.ass"), "zh-Hans");
        assert_eq!(detect_subtitle_language("Movie.srt"), "und");
    }
}
