//! Kodi-compatible NFO (XML) parser.
//!
//! Supports `<movie>`, `<tvshow>`, and `<episodedetails>` root elements.
//! Uses regex for lightweight parsing (no XML library dependency), matching
//! the approach used by the original TypeScript implementation.

use regex::Regex;

#[derive(Debug, Clone)]
pub struct NfoActor {
    pub name: String,
    pub role: Option<String>,
    pub thumb: Option<String>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct NfoInfo {
    pub nfo_type: NfoType,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    /// Runtime in minutes
    pub runtime: Option<i32>,
    pub tmdb_id: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub genres: Vec<String>,
    pub directors: Vec<String>,
    pub actors: Vec<NfoActor>,
    pub studio: Option<String>,
    pub country: Option<String>,
    pub rating: Option<f64>,
    pub content_rating: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub release_date: Option<String>,
    // streamdetails
    pub video_codec: Option<String>,
    pub video_width: Option<i32>,
    pub video_height: Option<i32>,
    pub video_profile: Option<String>,
    pub frame_rate: Option<f64>,
    pub duration_in_seconds: Option<i32>,
    pub video_bitrate: Option<i32>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_bitrate: Option<i32>,
    pub audio_languages: Vec<String>,
    pub hdr_type: Option<String>,
    pub vote_count: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum NfoType {
    Movie,
    TvShow,
    EpisodeDetails,
    #[default]
    Unknown,
}

impl NfoInfo {
    /// Whether the NFO has enough data to skip a TMDB API call.
    pub fn is_sufficient(&self) -> bool {
        self.title.as_ref().is_some_and(|t| !t.is_empty()) && self.plot.as_ref().is_some_and(|p| p.len() > 30)
    }
}

// ── Regex helpers (compiled once) ──

fn strip_cdata(raw: &str) -> String {
    let re = Regex::new(r"<!\[CDATA\[([\s\S]*?)\]\]>").unwrap();
    re.replace_all(raw, "$1").trim().to_string()
}

fn get_tag_text(xml: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?is)<{tag}(?:\s[^>]*)?>(.+?)</{tag}>");
    let re = Regex::new(&pattern).ok()?;
    let caps = re.captures(xml)?;
    let val = strip_cdata(&caps[1]);
    if val.is_empty() { None } else { Some(val) }
}

fn get_all_tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let pattern = format!(r"(?is)<{tag}(?:\s[^>]*)?>(.+?)</{tag}>");
    let Ok(re) = Regex::new(&pattern) else {
        return vec![];
    };
    re.captures_iter(xml)
        .filter_map(|caps| {
            let val = strip_cdata(&caps[1]);
            if val.is_empty() { None } else { Some(val) }
        })
        .collect()
}

fn get_unique_id(xml: &str, id_type: &str) -> Option<String> {
    // Standard: <uniqueid type="tmdb">12345</uniqueid>
    let pattern = format!(r#"(?is)<uniqueid[^>]*\btype="{id_type}"[^>]*>([^<]+)</uniqueid>"#);
    if let Some(caps) = Regex::new(&pattern).ok()?.captures(xml) {
        let val = caps[1].trim().to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }
    // Fallback: <uniqueid>12345</uniqueid><!-- tmdb --> (some generators put type in comment)
    let fallback = format!(r"(?is)<uniqueid[^>]*>([^<]+)</uniqueid>[^<]*<!--\s*{id_type}\s*-->");
    if let Some(caps) = Regex::new(&fallback).ok()?.captures(xml) {
        let val = caps[1].trim().to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}

fn safe_int(s: Option<String>) -> Option<i32> {
    s?.trim().parse().ok()
}

fn safe_float(s: Option<String>) -> Option<f64> {
    s?.trim().parse().ok()
}

fn parse_actors(xml: &str) -> Vec<NfoActor> {
    let re = Regex::new(r"(?is)<actor>([\s\S]*?)</actor>").unwrap();
    re.captures_iter(xml)
        .filter_map(|caps| {
            let block = &caps[1];
            let name = get_tag_text(block, "name")?;
            Some(NfoActor {
                name,
                role: get_tag_text(block, "role"),
                thumb: get_tag_text(block, "thumb"),
            })
        })
        .collect()
}

fn parse_stream_details(xml: &str) -> NfoStreamDetails {
    let mut sd = NfoStreamDetails::default();

    // Extract <fileinfo><streamdetails>...</streamdetails></fileinfo>
    let fi = match Regex::new(r"(?is)<fileinfo[^>]*>([\s\S]*?)</fileinfo>")
        .ok()
        .and_then(|re| re.captures(xml))
    {
        Some(caps) => caps[1].to_string(),
        None => return sd,
    };

    let sd_content = match Regex::new(r"(?is)<streamdetails[^>]*>([\s\S]*?)</streamdetails>")
        .ok()
        .and_then(|re| re.captures(&fi))
    {
        Some(caps) => caps[1].to_string(),
        None => return sd,
    };

    // Video stream (first one)
    if let Some(caps) = Regex::new(r"(?is)<video[^>]*>([\s\S]*?)</video>")
        .ok()
        .and_then(|re| re.captures(&sd_content))
    {
        let v = &caps[1];
        sd.video_codec = get_tag_text(v, "codec");
        sd.video_width = safe_int(get_tag_text(v, "width"));
        sd.video_height = safe_int(get_tag_text(v, "height"));
        sd.video_profile = get_tag_text(v, "profile");
        sd.frame_rate = safe_float(get_tag_text(v, "framerate"));
        let dur_secs = safe_int(get_tag_text(v, "durationinseconds"));
        let dur_mins = safe_float(get_tag_text(v, "duration"));
        if dur_secs.is_some_and(|d| d > 0) {
            sd.duration_in_seconds = dur_secs;
        } else if let Some(mins) = dur_mins
            && mins > 0.0
        {
            sd.duration_in_seconds = Some((mins * 60.0).round() as i32);
        }
        sd.video_bitrate = safe_int(get_tag_text(v, "bitrate")).filter(|&b| b > 0);
        sd.hdr_type = get_tag_text(v, "hdrtype").map(|s| s.to_lowercase());
    }

    // Audio streams (all of them)
    if let Ok(re) = Regex::new(r"(?is)<audio[^>]*>([\s\S]*?)</audio>") {
        let mut is_first = true;
        for caps in re.captures_iter(&sd_content) {
            let a = &caps[1];
            if is_first {
                sd.audio_codec = get_tag_text(a, "codec");
                sd.audio_channels = safe_int(get_tag_text(a, "channels"));
                sd.audio_bitrate = safe_int(get_tag_text(a, "bitrate")).filter(|&b| b > 0);
                is_first = false;
            }
            if let Some(lang) = get_tag_text(a, "language") {
                let lang = lang.trim().to_string();
                if !lang.is_empty() && !sd.audio_languages.contains(&lang) {
                    sd.audio_languages.push(lang);
                }
            }
        }
    }

    sd
}

#[derive(Debug, Default)]
struct NfoStreamDetails {
    video_codec: Option<String>,
    video_width: Option<i32>,
    video_height: Option<i32>,
    video_profile: Option<String>,
    frame_rate: Option<f64>,
    duration_in_seconds: Option<i32>,
    video_bitrate: Option<i32>,
    audio_codec: Option<String>,
    audio_channels: Option<i32>,
    audio_bitrate: Option<i32>,
    audio_languages: Vec<String>,
    hdr_type: Option<String>,
}

// ── Main entry ──

pub fn parse_nfo(xml: &str) -> NfoInfo {
    let nfo_type = if Regex::new(r"(?i)<movie[\s>]").ok().is_some_and(|re| re.is_match(xml)) {
        NfoType::Movie
    } else if Regex::new(r"(?i)<tvshow[\s>]").ok().is_some_and(|re| re.is_match(xml)) {
        NfoType::TvShow
    } else if Regex::new(r"(?i)<episodedetails[\s>]")
        .ok()
        .is_some_and(|re| re.is_match(xml))
    {
        NfoType::EpisodeDetails
    } else {
        NfoType::Unknown
    };

    let actors = parse_actors(xml);

    // TMDB ID: <uniqueid type="tmdb"> → <tmdbid> → <id> (pure digits only)
    let tmdb_id = get_unique_id(xml, "tmdb")
        .or_else(|| get_tag_text(xml, "tmdbid").filter(|v| v.chars().all(|c| c.is_ascii_digit())))
        .or_else(|| {
            // Only use <id> if value is pure digits (assumed TMDB)
            get_tag_text(xml, "id").filter(|v| v.chars().all(|c| c.is_ascii_digit()))
        });

    // IMDb ID: <uniqueid type="imdb"> → <imdbid>
    let imdb_id = get_unique_id(xml, "imdb").or_else(|| {
        get_tag_text(xml, "imdbid").filter(|v| v.starts_with("tt") && v[2..].chars().all(|c| c.is_ascii_digit()))
    });

    // TVDb ID: <uniqueid type="tvdb"> → <tvdbid>
    let tvdb_id = get_unique_id(xml, "tvdb")
        .or_else(|| get_tag_text(xml, "tvdbid").filter(|v| v.chars().all(|c| c.is_ascii_digit())));

    // Poster URL: <thumb aspect="poster">URL</thumb>
    let poster_url = Regex::new(r#"(?i)<thumb[^>]*\baspect="poster"[^>]*>([^<]+)</thumb>"#)
        .ok()
        .and_then(|re| re.captures(xml))
        .and_then(|caps| {
            let url = caps[1].trim();
            url.starts_with("http").then(|| url.to_string())
        });

    // Backdrop URL: <fanart><thumb>URL</thumb></fanart>
    let backdrop_url = Regex::new(r"(?is)<fanart[^>]*>([\s\S]*?)</fanart>")
        .ok()
        .and_then(|re| re.captures(xml))
        .and_then(|caps| {
            Regex::new(r"(?i)<thumb[^>]*>([^<]+)</thumb>")
                .ok()
                .and_then(|re2| re2.captures(&caps[1]))
                .and_then(|caps2| {
                    let url = caps2[1].trim();
                    url.starts_with("http").then(|| url.to_string())
                })
        });

    let release_date = get_tag_text(xml, "premiered").or_else(|| get_tag_text(xml, "releasedate"));

    let sd = parse_stream_details(xml);

    NfoInfo {
        nfo_type,
        title: get_tag_text(xml, "title"),
        original_title: get_tag_text(xml, "originaltitle"),
        year: safe_int(get_tag_text(xml, "year")),
        plot: get_tag_text(xml, "plot"),
        tagline: get_tag_text(xml, "tagline"),
        runtime: safe_int(get_tag_text(xml, "runtime")),
        tmdb_id,
        imdb_id,
        tvdb_id,
        season: safe_int(get_tag_text(xml, "season")),
        episode: safe_int(get_tag_text(xml, "episode")),
        genres: get_all_tag_texts(xml, "genre"),
        directors: get_all_tag_texts(xml, "director"),
        actors,
        studio: get_tag_text(xml, "studio"),
        country: get_tag_text(xml, "country"),
        rating: safe_float(get_tag_text(xml, "rating")),
        content_rating: get_tag_text(xml, "mpaa").or_else(|| get_tag_text(xml, "certification")),
        poster_url,
        backdrop_url,
        release_date,
        video_codec: sd.video_codec,
        video_width: sd.video_width,
        video_height: sd.video_height,
        video_profile: sd.video_profile,
        frame_rate: sd.frame_rate,
        duration_in_seconds: sd.duration_in_seconds,
        video_bitrate: sd.video_bitrate,
        audio_codec: sd.audio_codec,
        audio_channels: sd.audio_channels,
        audio_bitrate: sd.audio_bitrate,
        audio_languages: sd.audio_languages,
        hdr_type: sd.hdr_type,
        vote_count: safe_int(get_tag_text(xml, "votes")),
    }
}

/// Extract TMDB image path from a full URL.
/// e.g. "<https://image.tmdb.org/t/p/w500/abc123.jpg>" → "/abc123.jpg"
pub fn extract_tmdb_path(url: Option<&str>) -> Option<String> {
    let url = url?;
    let re = Regex::new(r"image\.tmdb\.org/t/p/[^/]+(/[^?]+)").unwrap();
    re.captures(url).map(|caps| caps[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pornhub_nfo() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<movie>
  <title>Perfect Tits of My Stepsis Bouncing When We Are Home Alone.</title>
  <originaltitle>Perfect Tits of My Stepsis Bouncing When We Are Home Alone.</originaltitle>
  <sorttitle>Perfect Tits of My Stepsis Bouncing When We Are Home Alone.</sorttitle>
  <plot><![CDATA[Perfect Tits of My Stepsis Bouncing When We Are Home Alone.]]></plot>
  <studio>Pornhub</studio>
  <director>Mia Luv</director>
  <runtime>579</runtime>
  <year>2023</year>
  <premiered>2023-01-10</premiered>
  <uniqueid type="provider" default="true">pornhub</uniqueid>
  <uniqueid type="pornhub">ph63bddb64da7ad</uniqueid>
  <sourceurl>https://www.pornhub.com/view_video.php?viewkey=ph63bddb64da7ad</sourceurl>
  <thumb aspect="poster">https://ei.phncdn.com/videos/202301/10/423084592/thumbs_10/(m=eaAaGwObaaaa)(mh=QMoUc5FPD4qJnOas)1.jpg</thumb>
</movie>"#;

        let nfo = parse_nfo(xml);
        assert_eq!(
            nfo.title,
            Some("Perfect Tits of My Stepsis Bouncing When We Are Home Alone.".to_string())
        );
    }
}
