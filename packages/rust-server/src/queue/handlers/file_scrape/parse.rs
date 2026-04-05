//! Filename and season/episode parsing for media files.
//! Aligned with TS `media-parser.ts`: CJK title extraction, CJK season/episode,
//! multi-episode range, parent dir season inference, bare E01/EP01.

use regex::Regex;
use std::sync::LazyLock;

// ── Regex patterns (compiled once) ──

/// S01E02, S01E02-E05, S01E02E03
static RE_SEASON_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S(\d{1,2})E(\d{1,4})(?:[-–]?E(\d{1,4}))?").unwrap());

/// `NxNN` format: 1x02, 2x05
static RE_NX_NN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[.\s\-])(\d{1,2})x(\d{1,4})(?:[.\s\-]|$)").unwrap());

/// Multi-episode range: E01-E05, E01-05
static RE_MULTI_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)E(\d{1,4})[-–]E?(\d{1,4})").unwrap());

/// Season only: S01, Season 1, Season.1
static RE_SEASON_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:S|Season[\s.]?)(\d{1,2})(?:\D|$)").unwrap());

/// Episode only: E01, EP01, EP.01
static RE_EPISODE_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:E|EP[\s.]?)(\d{1,4})").unwrap());

/// Year in brackets/separators
static RE_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[\s.\(\[\-])(\d{4})(?:[\s.\)\]\-]|$)").unwrap());

/// CJK season: 第1季, 第一季, 第01季
static RE_CJK_SEASON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第\s*([0-9零一二三四五六七八九十百]+)\s*季").unwrap());

/// CJK episode: 第1集, 第01集, 第一话, 第01話
static RE_CJK_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第\s*([0-9零一二三四五六七八九十百]+)\s*[集话話期]").unwrap());

/// Clean CJK title tail: strip season/episode indicators
static RE_CJK_TITLE_CLEAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[\s.]+(?:S\d|第\s*\d|Season|EP?\d).*$").unwrap());

/// Strips season/episode suffix from a space-normalized title (fallback path).
/// e.g. "Ever Night s01 e37" → "Ever Night", "Show S01E02 720p" → "Show"
static RE_TITLE_SE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(?:s\d{1,2}(?:[\s.]*e\d{1,4})?|ep?\s*\d{1,4}).*$").unwrap()
});

// ── CJK detection helpers ──

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'     // CJK Unified
        | '\u{3400}'..='\u{4DBF}'   // CJK Extension A
        | '\u{3040}'..='\u{309F}'   // Hiragana
        | '\u{30A0}'..='\u{30FF}'   // Katakana
        | '\u{AC00}'..='\u{D7AF}'   // Hangul
        | '\u{F900}'..='\u{FAFF}'   // CJK Compat
        | '\u{20000}'..='\u{2A6DF}' // CJK Extension B
    )
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk_char)
}

/// CJK number map: 零→0, 一→1 ... 十→10, 百→100
fn parse_cjk_number(s: &str) -> Option<i32> {
    if s.chars().all(|c| c.is_ascii_digit()) {
        return s.parse().ok();
    }
    let map = |c| match c {
        '零' => Some(0), '一' => Some(1), '二' => Some(2), '三' => Some(3),
        '四' => Some(4), '五' => Some(5), '六' => Some(6), '七' => Some(7),
        '八' => Some(8), '九' => Some(9), '十' => Some(10), '百' => Some(100),
        c if c.is_ascii_digit() => c.to_digit(10).map(|d| d as i32),
        _ => None,
    };
    let mut result = 0i32;
    let mut current = 0i32;
    for c in s.chars() {
        let Some(val) = map(c) else { continue };
        if val == 10 {
            result += (if current == 0 { 1 } else { current }) * 10;
            current = 0;
        } else if val == 100 {
            result += (if current == 0 { 1 } else { current }) * 100;
            current = 0;
        } else {
            current = val;
        }
    }
    result += current;
    if result > 0 { Some(result) } else { None }
}

// ── Parsed result ──

/// Full parsed media info aligned with TS `ParsedMediaInfo`.
pub struct ParsedMediaInfo {
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episodes: Option<Vec<i32>>,
}

// ── Main parse function ──

/// Parse media filename, extracting title, year, season, episode.
/// `parent_dir`: optional parent directory name for season inference.
pub fn parse_media_filename(filename: &str, parent_dir: Option<&str>) -> ParsedMediaInfo {
    let ext_pos = filename.rfind('.');
    let name = match ext_pos {
        Some(pos) if pos > 0 && pos > filename.len().saturating_sub(8) => &filename[..pos],
        _ => filename,
    };

    // 1. Basic title + year extraction
    let (mut title, mut year) = extract_title_and_year(name);

    // 2. CJK title extraction (if filename contains CJK but extracted title is all ASCII)
    if has_cjk(name)
        && let Some(cjk_title) = extract_cjk_title(name)
            && !cjk_title.is_empty() {
                title = cjk_title;
            }

    // 3. Season / Episode parsing (enhanced with CJK)
    let (mut season, mut episodes) = (None::<i32>, None::<Vec<i32>>);

    // Standard SxxEyy
    if let Some(caps) = RE_SEASON_EPISODE.captures(name) {
        season = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let ep_start: Option<i32> = caps.get(2).and_then(|m| m.as_str().parse().ok());
        let ep_end: Option<i32> = caps.get(3).and_then(|m| m.as_str().parse().ok());
        if let (Some(start), Some(end)) = (ep_start, ep_end) {
            episodes = Some((start..=end).collect());
        } else if let Some(ep) = ep_start {
            episodes = Some(vec![ep]);
        }
    }

    // NxNN format: 1x02
    if season.is_none()
        && let Some(caps) = RE_NX_NN.captures(name) {
            season = caps.get(1).and_then(|m| m.as_str().parse().ok());
            if let Some(ep) = caps.get(2).and_then(|m| m.as_str().parse::<i32>().ok()) {
                episodes = Some(vec![ep]);
            }
        }

    // Multi-episode range: E01-E05
    if episodes.as_ref().is_none_or(|e| e.len() <= 1)
        && let Some(caps) = RE_MULTI_EPISODE.captures(name) {
            let start: Option<i32> = caps.get(1).and_then(|m| m.as_str().parse().ok());
            let end: Option<i32> = caps.get(2).and_then(|m| m.as_str().parse().ok());
            if let (Some(s), Some(e)) = (start, end) {
                episodes = Some((s..=e).collect());
            }
        }

    // CJK season: 第X季
    if season.is_none()
        && let Some(caps) = RE_CJK_SEASON.captures(name) {
            season = caps.get(1).and_then(|m| parse_cjk_number(m.as_str()));
        }

    // CJK episode: 第X集/话/話/期
    if episodes.is_none()
        && let Some(caps) = RE_CJK_EPISODE.captures(name)
            && let Some(ep) = caps.get(1).and_then(|m| parse_cjk_number(m.as_str())) {
                episodes = Some(vec![ep]);
            }

    // Parent dir season inference: "Season 1", "S01", "第1季"
    if season.is_none()
        && let Some(pdir) = parent_dir {
            if let Some(caps) = RE_SEASON_ONLY.captures(pdir) {
                season = caps.get(1).and_then(|m| m.as_str().parse().ok());
            }
            if season.is_none()
                && let Some(caps) = RE_CJK_SEASON.captures(pdir) {
                    season = caps.get(1).and_then(|m| parse_cjk_number(m.as_str()));
                }
        }

    // Season only (no episode) from filename
    if season.is_none()
        && let Some(caps) = RE_SEASON_ONLY.captures(name) {
            season = caps.get(1).and_then(|m| m.as_str().parse().ok());
        }

    // Bare episode: E01, EP01
    if episodes.is_none()
        && let Some(caps) = RE_EPISODE_ONLY.captures(name)
            && let Some(ep) = caps.get(1).and_then(|m| m.as_str().parse().ok()) {
                episodes = Some(vec![ep]);
            }

    // 4. Year fallback
    if year.is_none()
        && let Some(caps) = RE_YEAR.captures(name)
            && let Some(m) = caps.get(1)
                && let Ok(y) = m.as_str().parse::<i32>()
                    && (1900..=2100).contains(&y) {
                        year = Some(y);
                    }

    ParsedMediaInfo { title, year, season, episodes }
}

/// Extract CJK title from brackets or filename start.
fn extract_cjk_title(name: &str) -> Option<String> {
    // Try brackets first: 【CJK...】 [CJK...] 「CJK...」 （CJK...） (CJK...)
    let bracket_pairs: &[(char, char)] = &[
        ('【', '】'), ('[', ']'), ('「', '」'), ('（', '）'), ('(', ')'),
    ];
    for &(open, close) in bracket_pairs {
        if let Some(start) = name.find(open) {
            let after = &name[start + open.len_utf8()..];
            if let Some(end) = after.find(close) {
                let content = &after[..end];
                if has_cjk(content) {
                    let cleaned = RE_CJK_TITLE_CLEAN.replace(content, "").trim().to_string();
                    if !cleaned.is_empty() {
                        return Some(cleaned);
                    }
                }
            }
        }
    }

    // Try filename start: leading CJK characters
    let mut last_cjk_or_digit_idx = 0;
    let mut found_cjk = false;
    for (i, c) in name.char_indices() {
        if is_cjk_char(c) {
            found_cjk = true;
            last_cjk_or_digit_idx = i + c.len_utf8();
        } else if found_cjk && (c.is_ascii_digit() || c.is_ascii_alphabetic()
            || c == ' ' || c == '·' || c == '：' || c == ':' || c == '—'
            || c == '-' || c == '~' || c == '～')
        {
            if c.is_ascii_digit() || is_cjk_char(c) {
                last_cjk_or_digit_idx = i + c.len_utf8();
            }
        } else {
            break;
        }
    }
    if found_cjk && last_cjk_or_digit_idx > 0 {
        let raw = &name[..last_cjk_or_digit_idx];
        let cleaned = RE_CJK_TITLE_CLEAN.replace(raw, "").trim().to_string();
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

fn extract_title_and_year(name: &str) -> (String, Option<i32>) {
    if let Some(result) = extract_year_in_brackets(name) {
        return result;
    }
    if let Some(result) = extract_year_with_dots(name) {
        return result;
    }
    if let Some(result) = extract_year_with_spaces(name) {
        return result;
    }
    let clean = name.replace(['.', '_'], " ");
    let title = RE_TITLE_SE_SUFFIX.replace(&clean, "").trim().to_string();
    (title, None)
}

fn extract_year_in_brackets(name: &str) -> Option<(String, Option<i32>)> {
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some(pos) = name.rfind(open) {
            let after = &name[pos + 1..];
            if let Some(end) = after.find(close)
                && let Some(year) = parse_year_str(&after[..end]) {
                    let title = name[..pos].trim();
                    if !title.is_empty() {
                        return Some((title.to_string(), Some(year)));
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
    if s.len() == 4
        && let Ok(y) = s.parse::<i32>()
            && (1900..=2099).contains(&y) {
                return Some(y);
            }
    None
}

// ── Legacy wrappers (used by mod.rs) ──

/// Extract season and episode from filename.
/// Returns first episode only for backward compat; use `parse_media_filename` for multi-episode.
#[allow(dead_code)]
pub fn parse_season_episode(filename: &str) -> Option<(i32, i32)> {
    let info = parse_media_filename(filename, None);
    match (info.season, info.episodes) {
        (Some(s), Some(eps)) if !eps.is_empty() => Some((s, eps[0])),
        _ => None,
    }
}

/// Check whether the filename looks like a Blu-ray disc placeholder.
/// Aligned with TS: 4-5 digit stems only (not 4+).
pub fn is_placeholder_disc_stem(filename: &str, parsed_title: &str) -> bool {
    let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "m2ts" | "mts" | "vob") {
        return false;
    }
    let t = parsed_title.trim();
    (4..=5).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit())
}

/// Detect subtitle language from filename.
pub fn detect_subtitle_language(filename: &str) -> String {
    let without_ext = filename.rsplit_once('.').map_or(filename, |(s, _)| s);
    let parts: Vec<&str> = without_ext.split('.').collect();
    if let Some(&last) = parts.last()
        && last != without_ext && last.len() >= 2 && last.len() <= 16 {
            let is_lang = last.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if is_lang && !last.chars().all(|c| c.is_ascii_digit()) {
                return last.to_string();
            }
        }
    "und".to_string()
}

/// Find a poster file named `{stem}.{ext}` among directory entries.
pub fn find_stem_poster_filename(dir_entries: &[String], stem: &str) -> Option<String> {
    let lower_stem = stem.to_ascii_lowercase();
    for ext in super::constants::POSTER_EXTENSIONS {
        let candidate = format!("{lower_stem}.{ext}");
        // Return the lowercased candidate (aligned with TS which returns the candidate string)
        if dir_entries.iter().any(|e| e.to_ascii_lowercase() == candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename_year_in_parens() {
        let info = parse_media_filename("The Matrix (1999).mkv", None);
        assert_eq!(info.title, "The Matrix");
        assert_eq!(info.year, Some(1999));
    }

    #[test]
    fn test_parse_filename_year_in_brackets() {
        let info = parse_media_filename("Movie [2024].mp4", None);
        assert_eq!(info.title, "Movie");
        assert_eq!(info.year, Some(2024));
    }

    #[test]
    fn test_parse_filename_year_with_dots() {
        let info = parse_media_filename("movie.2024.1080p.BluRay.mkv", None);
        assert_eq!(info.title, "movie");
        assert_eq!(info.year, Some(2024));
    }

    #[test]
    fn test_parse_filename_no_year() {
        let info = parse_media_filename("some_movie.mkv", None);
        assert_eq!(info.title, "some movie");
        assert_eq!(info.year, None);
    }

    #[test]
    fn test_parse_season_episode_standard() {
        let info = parse_media_filename("show.S01E02.720p.mkv", None);
        assert_eq!(info.season, Some(1));
        assert_eq!(info.episodes, Some(vec![2]));
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

    // ── CJK tests ──

    #[test]
    fn test_cjk_title_in_brackets() {
        let info = parse_media_filename("[哪吒之魔童闹海].Erta.2025.mkv", None);
        assert_eq!(info.title, "哪吒之魔童闹海");
    }

    #[test]
    fn test_cjk_title_start() {
        let info = parse_media_filename("哪吒之魔童闹海 (2025) 1080p.mkv", None);
        assert_eq!(info.title, "哪吒之魔童闹海");
    }

    #[test]
    fn test_cjk_season_episode() {
        let info = parse_media_filename("进击的巨人 第三季 第01集.mkv", None);
        assert_eq!(info.season, Some(3));
        assert_eq!(info.episodes, Some(vec![1]));
    }

    #[test]
    fn test_multi_episode_range() {
        let info = parse_media_filename("show.S01E01-E05.720p.mkv", None);
        assert_eq!(info.season, Some(1));
        assert_eq!(info.episodes, Some(vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_bare_episode() {
        let info = parse_media_filename("show EP03 720p.mkv", None);
        assert_eq!(info.episodes, Some(vec![3]));
    }

    #[test]
    fn test_parent_dir_season() {
        let info = parse_media_filename("E05.720p.mkv", Some("Season 2"));
        assert_eq!(info.season, Some(2));
        assert_eq!(info.episodes, Some(vec![5]));
    }

    #[test]
    fn test_parent_dir_cjk_season() {
        let info = parse_media_filename("第05集.mkv", Some("第二季"));
        assert_eq!(info.season, Some(2));
        assert_eq!(info.episodes, Some(vec![5]));
    }

    #[test]
    fn test_cjk_number_parsing() {
        assert_eq!(parse_cjk_number("三"), Some(3));
        assert_eq!(parse_cjk_number("十二"), Some(12));
        assert_eq!(parse_cjk_number("二十五"), Some(25));
        assert_eq!(parse_cjk_number("01"), Some(1));
    }
}
