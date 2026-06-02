use std::collections::HashMap;

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement, Value};
use serde_json::json;

use uuid::Uuid;

use crate::error::AppError;
use crate::error::OptionExt;

fn normalize_subtitle_codec(s: &str) -> String {
    s.to_string()
}

/// Input for listing movies or TV shows.
#[derive(Debug)]
pub struct ListMediaInput {
    pub video_id: Uuid,
    pub page: i64,
    pub page_size: i64,
    pub sort_by: String,
    pub sort_dir: String,
    pub genre_id: Option<Uuid>,
    pub search: Option<String>,
    pub country: Option<String>,
    pub favorite: Option<bool>,
    pub resolution: Option<String>,
    pub runtime: Option<String>,
}

/// Input for listing albums.
#[derive(Debug)]
pub struct ListAlbumsInput {
    pub music_id: Uuid,
    pub page: i64,
    pub page_size: i64,
    pub sort_by: String,
    pub sort_dir: String,
    pub genre: Option<String>,
    pub search: Option<String>,
    pub artist_id: Option<Uuid>,
    pub favorite: Option<bool>,
}

/// Input for listing tracks.
#[derive(Debug)]
pub struct ListTracksInput {
    pub music_id: Uuid,
    pub page: i64,
    pub page_size: i64,
    pub sort_by: String,
    pub sort_dir: String,
    pub genre: Option<String>,
    pub search: Option<String>,
}

// ── Helpers ──

fn col<T: sea_orm::TryGetable>(r: &sea_orm::QueryResult, c: &str) -> Result<T, AppError> {
    r.try_get::<T>("", c)
        .map_err(|e| AppError::Internal(format!("col '{c}': {e:?}")))
}

/// Resolve a TMDB genre ID to its English name (used for API output).
fn tmdb_genre_name(id: i32) -> &'static str {
    match id {
        12 => "Adventure",
        14 => "Fantasy",
        16 => "Animation",
        18 => "Drama",
        27 => "Horror",
        28 => "Action",
        35 => "Comedy",
        36 => "History",
        37 => "Western",
        53 => "Thriller",
        80 => "Crime",
        99 => "Documentary",
        878 => "Science Fiction",
        9648 => "Mystery",
        10402 => "Music",
        10749 => "Romance",
        10751 => "Family",
        10752 => "War",
        10759 => "Action & Adventure",
        10762 => "Kids",
        10763 => "News",
        10764 => "Reality",
        10765 => "Sci-Fi & Fantasy",
        10766 => "Soap",
        10767 => "Talk",
        10768 => "War & Politics",
        10770 => "TV Movie",
        _ => "Unknown",
    }
}

fn opt<T: sea_orm::TryGetable>(r: &sea_orm::QueryResult, c: &str) -> Option<T> {
    r.try_get::<Option<T>>("", c).ok().flatten()
}

fn dir(d: &str) -> &'static str {
    if d.eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" }
}

fn video_item_order(s: &str) -> &'static str {
    match s {
        "year" => "m.year",
        "rating" => "COALESCE(m.tmdb_rating, m.imdb_rating)",
        "addedAt" | "createdAt" => "m.created_at",
        _ => "m.title",
    }
}

fn build_source_address(fs_type: Option<&str>, fs_config: Option<&serde_json::Value>) -> Option<String> {
    let config = fs_config?;
    let typ = fs_type?;
    match typ {
        "smb" => {
            let host = config.get("host")?.as_str()?;
            let share = config.get("share").and_then(|v| v.as_str()).unwrap_or("");
            // Multi-share config (e.g. "r18,*") — show host only
            if share.is_empty() || share.contains(',') || share.contains('*') {
                Some(format!("smb://{host}"))
            } else {
                Some(format!("smb://{host}/{share}"))
            }
        }
        "nfs" => {
            let host = config.get("host")?.as_str()?;
            let export = config.get("exportPath").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("nfs://{host}{export}"))
        }
        other => {
            if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                Some(url.to_string())
            } else {
                config
                    .get("host")
                    .and_then(|v| v.as_str())
                    .map(|host| format!("{other}://{host}"))
            }
        }
    }
}

fn file_row_to_json(r: &sea_orm::QueryResult) -> Result<serde_json::Value, AppError> {
    let fs_type_val: Option<String> = opt(r, "fs_type");
    let fs_config_val: Option<serde_json::Value> = opt(r, "fs_config");
    let source_name: Option<String> = opt(r, "fs_name");
    let source_type = fs_type_val.clone();
    let source_address = build_source_address(fs_type_val.as_deref(), fs_config_val.as_ref());

    Ok(json!({
        "id": col::<Uuid>(r, "id")?.to_string(),
        "path": col::<String>(r, "path")?,
        "filename": col::<String>(r, "filename")?,
        "size": opt::<i64>(r, "size"),
        "mimeType": opt::<String>(r, "mime_type"),
        "duration": opt::<i32>(r, "duration"),
        "checksum": opt::<String>(r, "checksum"),
        "videoCodec": opt::<String>(r, "video_codec"),
        "videoWidth": opt::<i32>(r, "video_width"),
        "videoHeight": opt::<i32>(r, "video_height"),
        "videoProfile": opt::<String>(r, "video_profile"),
        "hdrType": opt::<String>(r, "hdr_type"),
        "audioStreams": opt::<serde_json::Value>(r, "audio_streams"),
        "videoStreams": opt::<serde_json::Value>(r, "video_streams"),
        "isAvailable": col::<bool>(r, "is_available").unwrap_or(true),
        "ffprobeRaw": opt::<serde_json::Value>(r, "ffprobe_raw"),
        "scannedAt": opt::<String>(r, "file_scanned_at"),
        "createdAt": opt::<String>(r, "file_created_at"),
        "updatedAt": opt::<String>(r, "file_updated_at"),
        "sourceName": source_name,
        "sourceType": source_type,
        "sourceAddress": source_address,
    }))
}

fn subtitle_to_json(r: &sea_orm::QueryResult) -> serde_json::Value {
    let s3_key: Option<String> = opt(r, "s3_key");
    let storage_url = s3_key.as_ref().map(|k| format!("/storage/{k}"));
    json!({
        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
        "language": col::<String>(r, "language").unwrap_or_default(),
        "title": opt::<String>(r, "title"),
        "sourceType": col::<String>(r, "source_type").unwrap_or_default(),
        "format": normalize_subtitle_codec(&col::<String>(r, "format").unwrap_or_default()),
        "path": opt::<String>(r, "path"),
        "storageUrl": storage_url,
        "source": opt::<String>(r, "source"),
        "isDefault": col::<bool>(r, "is_default").unwrap_or(false),
        "isForced": col::<bool>(r, "is_forced").unwrap_or(false),
        "isHearingImpaired": col::<bool>(r, "is_hearing_impaired").unwrap_or(false),
    })
}

fn chapter_to_json(r: &sea_orm::QueryResult) -> serde_json::Value {
    json!({
        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
        "index": col::<i32>(r, "index").unwrap_or(0),
        "title": opt::<String>(r, "title"),
        "startTime": col::<i32>(r, "start_time").unwrap_or(0),
        "endTime": opt::<i32>(r, "end_time"),
        "thumbPath": opt::<String>(r, "thumb_path"),
    })
}

fn collection_to_json(r: &sea_orm::QueryResult) -> serde_json::Value {
    json!({
        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
        "name": col::<String>(r, "name").unwrap_or_default(),
        "posterPath": opt::<String>(r, "poster_path"),
        "overview": opt::<String>(r, "overview"),
    })
}

async fn query_count(db: &impl ConnectionTrait, sql: &str, params: Vec<Value>) -> Result<i64, AppError> {
    let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, params);
    let row = db.query_one_raw(stmt).await?.internal("count query returned no rows")?;
    col(&row, "total")
}

async fn query_subtitles_for_files(
    db: &impl ConnectionTrait,
    join_col: &str,
    parent_id: Uuid,
) -> Result<HashMap<String, Vec<serde_json::Value>>, AppError> {
    let sql = format!(
        "SELECT s.*, s.file_id, mf.id as mf_id FROM subtitles s \
         JOIN video_files mf ON mf.id = s.file_id \
         WHERE mf.{join_col} = $1 AND mf.is_available = true \
         ORDER BY s.file_id, s.created_at ASC, s.id ASC"
    );
    let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [parent_id.into()]);
    let rows = db.query_all_raw(stmt).await?;
    let mut map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for r in &rows {
        let fid = col::<Uuid>(r, "file_id").map(|v| v.to_string()).unwrap_or_default();
        map.entry(fid).or_default().push(subtitle_to_json(r));
    }
    Ok(map)
}

async fn query_chapters_for_files(
    db: &impl ConnectionTrait,
    join_col: &str,
    parent_id: Uuid,
) -> Result<HashMap<String, Vec<serde_json::Value>>, AppError> {
    let sql = format!(
        "SELECT c.*, c.file_id FROM chapters c \
         JOIN video_files mf ON mf.id = c.file_id \
         WHERE mf.{join_col} = $1 AND mf.is_available = true \
         ORDER BY c.file_id, c.index ASC"
    );
    let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [parent_id.into()]);
    let rows = db.query_all_raw(stmt).await?;
    let mut map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for r in &rows {
        let fid = col::<Uuid>(r, "file_id").map(|v| v.to_string()).unwrap_or_default();
        map.entry(fid).or_default().push(chapter_to_json(r));
    }
    Ok(map)
}

const FILE_SELECT: &str = "\
    mf.id, mf.path, mf.filename, mf.size, mf.mime_type, mf.duration, \
    mf.checksum, mf.video_codec, mf.video_width, mf.video_height, \
    mf.video_profile, mf.hdr_type, mf.is_available, \
    mf.audio_streams, mf.video_streams, mf.ffprobe_raw, \
    mf.scanned_at::text as file_scanned_at, \
    mf.created_at::text as file_created_at, mf.updated_at::text as file_updated_at, \
    fs.name as fs_name, fs.type as fs_type, fs.config as fs_config";

const FILE_JOINS: &str = "\
    LEFT JOIN vfs fs ON fs.id = mf.source_id";

// ── Resolution / runtime helpers ──

/// Map resolution label to (min_height, max_height) range.
fn resolution_range(label: &str) -> (i32, i32) {
    match label {
        "4k" => (2000, 100_000),
        "1080p" => (1000, 2000),
        "720p" => (700, 1000),
        "480p" => (0, 700),
        _ => (0, 100_000),
    }
}

/// Map runtime label to (min_minutes, max_minutes) range.
fn runtime_range(label: &str) -> (i32, Option<i32>) {
    match label {
        "short" => (0, Some(60)),    // < 1h
        "medium" => (60, Some(120)), // 1h-2h
        "long" => (120, Some(180)),  // 2h-3h
        "extra_long" => (180, None), // > 3h
        _ => (0, None),
    }
}

// ── MediaContentRepo ──

pub struct MediaContentRepo;

impl MediaContentRepo {
    // ── Video Items ──

    pub async fn list_video_items(
        db: &impl ConnectionTrait,
        input: ListMediaInput,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let mut conds = vec![
            "m.video_id = $1".to_string(),
            "EXISTS (SELECT 1 FROM video_files mf WHERE mf.video_item_id = m.id AND mf.is_available = true)"
                .to_string(),
        ];
        let mut params: Vec<Value> = vec![input.video_id.into()];
        let mut n = 2usize;

        if let Some(gid) = input.genre_id {
            conds.push(format!(
                r"EXISTS (SELECT 1 FROM video_genres WHERE video_item_id = m.id AND genre_id = ${n})"
            ));
            params.push(gid.into());
            n += 1;
        }
        if let Some(s) = input.search {
            conds.push(format!("(m.title ILIKE ${n} OR m.original_title ILIKE ${n})"));
            params.push(format!("%{s}%").into());
            n += 1;
        }
        if let Some(ref c) = input.country {
            conds.push(format!("${n} = ANY(m.countries)"));
            params.push(c.clone().into());
            n += 1;
        }
        if let Some(true) = input.favorite {
            conds.push("m.is_favorite = true".to_string());
        }
        if let Some(ref res) = input.resolution {
            let (min_h, max_h) = resolution_range(res);
            conds.push(format!(
                "EXISTS (SELECT 1 FROM video_files mf WHERE mf.video_item_id = m.id \
                 AND mf.is_available = true AND mf.video_height >= {min_h} AND mf.video_height < {max_h})"
            ));
        }
        if let Some(ref rt) = input.runtime {
            let (min_rt, max_rt) = runtime_range(rt);
            if let Some(max_val) = max_rt {
                conds.push(format!("m.runtime >= {min_rt} AND m.runtime < {max_val}"));
            } else {
                conds.push(format!("m.runtime >= {min_rt}"));
            }
        }

        let wh = conds.join(" AND ");
        let order = video_item_order(&input.sort_by);
        let d = dir(&input.sort_dir);

        let total = query_count(
            db,
            &format!("SELECT COUNT(*) as total FROM video_items m WHERE {wh}"),
            params.clone(),
        )
        .await?;

        let lim = n;
        let off = n + 1;
        let isql = format!(
            "SELECT m.id, m.video_id, m.title, m.original_title, m.year, \
             m.release_date::text as release_date, m.poster_path, m.backdrop_path, m.overview, \
             COALESCE(m.tmdb_rating, m.imdb_rating) as rating, \
             m.is_adult, m.is_favorite, m.scraped_at::text as scraped_at, \
             m.created_at::text as created_at, m.updated_at::text as updated_at \
             FROM video_items m WHERE {wh} ORDER BY {order} {d} NULLS LAST LIMIT ${lim} OFFSET ${off}"
        );
        let offset = (input.page - 1) * input.page_size;
        params.push(input.page_size.into());
        params.push(offset.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &isql, params);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(json!({
                    "id": col::<Uuid>(r, "id")?.to_string(),
                    "videoId": col::<Uuid>(r, "video_id")?.to_string(),
                    "title": col::<String>(r, "title")?,
                    "originalTitle": opt::<String>(r, "original_title"),
                    "year": opt::<i32>(r, "year"),
                    "releaseDate": opt::<String>(r, "release_date"),
                    "posterPath": opt::<String>(r, "poster_path"),
                    "backdropPath": opt::<String>(r, "backdrop_path"),
                    "overview": opt::<String>(r, "overview"),
                    "rating": opt::<f64>(r, "rating"),
                    "isAdult": col::<bool>(r, "is_adult").unwrap_or(false),
                    "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                    "scrapedAt": opt::<String>(r, "scraped_at"),
                    "createdAt": opt::<String>(r, "created_at"),
                    "updatedAt": opt::<String>(r, "updated_at"),
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok((items, total))
    }

    // ── TV Shows ──

    pub async fn list_tv_shows(
        db: &impl ConnectionTrait,
        input: ListMediaInput,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let mut conds = vec![
            "t.video_id = $1".to_string(),
            "EXISTS (SELECT 1 FROM episodes ep JOIN video_files mf ON mf.episode_id = ep.id \
             WHERE ep.tv_show_id = t.id AND mf.is_available = true)"
                .to_string(),
        ];
        let mut params: Vec<Value> = vec![input.video_id.into()];
        let mut n = 2usize;

        if let Some(gid) = input.genre_id {
            conds.push(format!(
                r"EXISTS (SELECT 1 FROM tv_show_genres WHERE tv_show_id = t.id AND genre_id = ${n})"
            ));
            params.push(gid.into());
            n += 1;
        }
        if let Some(s) = input.search {
            conds.push(format!("(t.title ILIKE ${n} OR t.original_title ILIKE ${n})"));
            params.push(format!("%{s}%").into());
            n += 1;
        }
        if let Some(ref c) = input.country {
            conds.push(format!("${n} = ANY(t.countries)"));
            params.push(c.clone().into());
            n += 1;
        }
        if let Some(true) = input.favorite {
            conds.push("t.is_favorite = true".to_string());
        }
        if let Some(ref res) = input.resolution {
            let (min_h, max_h) = resolution_range(res);
            conds.push(format!(
                "EXISTS (SELECT 1 FROM episodes ep JOIN video_files mf ON mf.episode_id = ep.id \
                 WHERE ep.tv_show_id = t.id AND mf.is_available = true \
                 AND mf.video_height >= {min_h} AND mf.video_height < {max_h})"
            ));
        }

        let wh = conds.join(" AND ");
        let order = match input.sort_by.as_str() {
            "year" => "t.year",
            "rating" => "COALESCE(t.tmdb_rating, t.imdb_rating)",
            "addedAt" | "createdAt" => "t.created_at",
            _ => "t.title",
        };
        let d = dir(&input.sort_dir);

        let total = query_count(
            db,
            &format!("SELECT COUNT(*) as total FROM tv_shows t WHERE {wh}"),
            params.clone(),
        )
        .await?;

        let lim = n;
        let off = n + 1;
        let isql = format!(
            "SELECT t.id, t.video_id, t.title, t.original_title, t.year, \
             t.first_air_date::text as first_air_date, t.poster_path, t.backdrop_path, t.overview, \
             COALESCE(t.tmdb_rating, t.imdb_rating) as rating, \
             t.is_adult, t.is_favorite, t.status, \
             t.created_at::text as created_at, t.updated_at::text as updated_at \
             FROM tv_shows t WHERE {wh} ORDER BY {order} {d} NULLS LAST LIMIT ${lim} OFFSET ${off}"
        );
        let offset_val = (input.page - 1) * input.page_size;
        params.push(input.page_size.into());
        params.push(offset_val.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &isql, params);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(json!({
                    "id": col::<Uuid>(r, "id")?.to_string(),
                    "videoId": col::<Uuid>(r, "video_id")?.to_string(),
                    "title": col::<String>(r, "title")?,
                    "originalTitle": opt::<String>(r, "original_title"),
                    "year": opt::<i32>(r, "year"),
                    "firstAirDate": opt::<String>(r, "first_air_date"),
                    "posterPath": opt::<String>(r, "poster_path"),
                    "backdropPath": opt::<String>(r, "backdrop_path"),
                    "overview": opt::<String>(r, "overview"),
                    "rating": opt::<f64>(r, "rating"),
                    "isAdult": col::<bool>(r, "is_adult").unwrap_or(false),
                    "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                    "status": opt::<String>(r, "status"),
                    "createdAt": opt::<String>(r, "created_at"),
                    "updatedAt": opt::<String>(r, "updated_at"),
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok((items, total))
    }

    // ── Genres ──

    pub async fn list_genres(db: &impl ConnectionTrait, app_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
        let type_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT type FROM videos WHERE id = $1
             UNION ALL
             SELECT type FROM musics WHERE id = $1
             LIMIT 1",
            [app_id.into()],
        );
        let lib_row = db.query_one_raw(type_stmt).await?.not_found("video/app not found")?;
        let lib_type: String = col(&lib_row, "type")?;

        let sql = if lib_type == "tv" || lib_type == "anime" {
            r"SELECT DISTINCT g.id, g.tmdb_genre_id FROM genres g
               JOIN tv_show_genres tg ON tg.genre_id = g.id
               JOIN tv_shows t ON t.id = tg.tv_show_id AND t.video_id = $1
               ORDER BY g.tmdb_genre_id"
        } else {
            r"SELECT DISTINCT g.id, g.tmdb_genre_id FROM genres g
               JOIN video_genres mg ON mg.genre_id = g.id
               JOIN video_items m ON m.id = mg.video_item_id AND m.video_id = $1
               ORDER BY g.tmdb_genre_id"
        };
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, [app_id.into()]);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                let tmdb_id = col::<i32>(r, "tmdb_genre_id").unwrap_or(0);
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "tmdbGenreId": tmdb_id,
                    "name": tmdb_genre_name(tmdb_id),
                })
            })
            .collect();
        Ok(items)
    }

    // ── Countries ──

    pub async fn list_countries(db: &impl ConnectionTrait, app_id: Uuid) -> Result<Vec<String>, AppError> {
        let type_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT type FROM videos WHERE id = $1 LIMIT 1",
            [app_id.into()],
        );
        let lib_row = db.query_one_raw(type_stmt).await?.not_found("video not found")?;
        let lib_type: String = col(&lib_row, "type")?;

        let sql = if lib_type == "tv" || lib_type == "anime" {
            "SELECT DISTINCT unnest(t.countries) as country FROM tv_shows t \
             WHERE t.video_id = $1 AND t.countries IS NOT NULL ORDER BY country"
        } else {
            "SELECT DISTINCT unnest(m.countries) as country FROM video_items m \
             WHERE m.video_id = $1 AND m.countries IS NOT NULL ORDER BY country"
        };
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, [app_id.into()]);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows.iter().filter_map(|r| col::<String>(r, "country").ok()).collect();
        Ok(items)
    }

    // ── Recently Added ──

    pub async fn get_recently_added(
        db: &impl ConnectionTrait,
        app_id: Uuid,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let type_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT type FROM videos WHERE id = $1
             UNION ALL
             SELECT type FROM musics WHERE id = $1
             LIMIT 1",
            [app_id.into()],
        );
        let lib_row = db.query_one_raw(type_stmt).await?.not_found("video/app not found")?;
        let lib_type: String = col(&lib_row, "type")?;

        if lib_type == "tv" || lib_type == "anime" {
            let stmt = Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT t.id, t.title, t.year, t.poster_path, \
                 COALESCE(t.tmdb_rating, t.imdb_rating) as rating, t.is_favorite \
                 FROM tv_shows t WHERE t.video_id = $1 \
                 AND EXISTS (SELECT 1 FROM episodes ep JOIN video_files mf ON mf.episode_id = ep.id \
                 WHERE ep.tv_show_id = t.id AND mf.is_available = true) \
                 ORDER BY t.created_at DESC NULLS LAST LIMIT $2",
                [app_id.into(), limit.into()],
            );
            let rows = db.query_all_raw(stmt).await?;
            Ok(rows
                .iter()
                .map(|r| {
                    json!({
                        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                        "title": col::<String>(r, "title").unwrap_or_default(),
                        "year": opt::<i32>(r, "year"),
                        "posterPath": opt::<String>(r, "poster_path"),
                        "rating": opt::<f64>(r, "rating"),
                        "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                        "type": "tv",
                    })
                })
                .collect())
        } else if lib_type == "music" {
            let stmt = Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT a.id, a.title, a.year, a.cover_path as poster_path, a.is_favorite \
                 FROM music_albums a WHERE a.music_id = $1 \
                 ORDER BY a.created_at DESC NULLS LAST LIMIT $2",
                [app_id.into(), limit.into()],
            );
            let rows = db.query_all_raw(stmt).await?;
            Ok(rows
                .iter()
                .map(|r| {
                    json!({
                        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                        "title": col::<String>(r, "title").unwrap_or_default(),
                        "year": opt::<i32>(r, "year"),
                        "posterPath": opt::<String>(r, "poster_path"),
                        "rating": serde_json::Value::Null,
                        "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                        "type": "album",
                    })
                })
                .collect())
        } else {
            let stmt = Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT m.id, m.title, m.year, m.poster_path, \
                 COALESCE(m.tmdb_rating, m.imdb_rating) as rating, m.is_favorite \
                 FROM video_items m WHERE m.video_id = $1 \
                 AND EXISTS (SELECT 1 FROM video_files mf WHERE mf.video_item_id = m.id AND mf.is_available = true) \
                 ORDER BY m.created_at DESC NULLS LAST LIMIT $2",
                [app_id.into(), limit.into()],
            );
            let rows = db.query_all_raw(stmt).await?;
            Ok(rows
                .iter()
                .map(|r| {
                    json!({
                        "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                        "title": col::<String>(r, "title").unwrap_or_default(),
                        "year": opt::<i32>(r, "year"),
                        "posterPath": opt::<String>(r, "poster_path"),
                        "rating": opt::<f64>(r, "rating"),
                        "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                        "type": "movie",
                    })
                })
                .collect())
        }
    }

    // ── Toggle Favorite ──

    pub async fn toggle_favorite(db: &impl ConnectionTrait, media_type: &str, id: Uuid) -> Result<bool, AppError> {
        let sql = match media_type {
            "movie" => "UPDATE video_items SET is_favorite = NOT is_favorite WHERE id = $1 RETURNING is_favorite",
            "tvshow" | "tv" => "UPDATE tv_shows SET is_favorite = NOT is_favorite WHERE id = $1 RETURNING is_favorite",
            _ => {
                return Err(AppError::BadRequest(format!("invalid media type: {media_type}")));
            }
        };
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, [id.into()]);
        let row = db.query_one_raw(stmt).await?.not_found("item not found")?;
        col(&row, "is_favorite")
    }

    // ── Video Item Detail ──

    pub async fn get_video_item_detail(
        db: &impl ConnectionTrait,
        id: Uuid,
    ) -> Result<Option<serde_json::Value>, AppError> {
        // Base movie
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT m.id, m.video_id, m.title, m.original_title, m.sort_title, \
             m.year, m.release_date::text as release_date, m.runtime, \
             m.tmdb_rating, m.imdb_rating, m.douban_rating, \
             COALESCE(m.tmdb_rating, m.imdb_rating) as rating, \
             m.tmdb_id, m.imdb_id, m.douban_id, m.jav_number, \
             m.poster_path, m.backdrop_path, m.overview, m.tagline, \
             m.is_adult, m.is_favorite, m.original_language, m.content_rating, \
             m.metadata, \
             m.scraped_at::text as scraped_at, \
             m.created_at::text as created_at, m.updated_at::text as updated_at \
             FROM video_items m WHERE m.id = $1",
            [id.into()],
        );
        let Some(m) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        // Genres
        let genres = Self::query_genres_for_video_item(db, id).await?;
        // Credits
        let credits = Self::query_video_item_credits(db, id).await?;
        // Files with source info
        let files = Self::query_files_with_nested(db, "video_item_id", id).await?;
        // Collections
        let colls = Self::query_video_item_collections(db, id).await?;

        Ok(Some(json!({
            "id": col::<Uuid>(&m, "id")?.to_string(),
            "videoId": col::<Uuid>(&m, "video_id")?.to_string(),
            "title": col::<String>(&m, "title")?,
            "originalTitle": opt::<String>(&m, "original_title"),
            "sortTitle": opt::<String>(&m, "sort_title"),
            "year": opt::<i32>(&m, "year"),
            "releaseDate": opt::<String>(&m, "release_date"),
            "runtime": opt::<i32>(&m, "runtime"),
            "rating": opt::<f64>(&m, "rating"),
            "tmdbRating": opt::<f64>(&m, "tmdb_rating"),
            "imdbRating": opt::<f64>(&m, "imdb_rating"),
            "doubanRating": opt::<f64>(&m, "douban_rating"),
            "tmdbId": opt::<String>(&m, "tmdb_id"),
            "imdbId": opt::<String>(&m, "imdb_id"),
            "doubanId": opt::<String>(&m, "douban_id"),
            "javNumber": opt::<String>(&m, "jav_number"),
            "posterPath": opt::<String>(&m, "poster_path"),
            "backdropPath": opt::<String>(&m, "backdrop_path"),
            "overview": opt::<String>(&m, "overview"),
            "tagline": opt::<String>(&m, "tagline"),
            "isAdult": col::<bool>(&m, "is_adult").unwrap_or(false),
            "isFavorite": col::<bool>(&m, "is_favorite").unwrap_or(false),
            "originalLanguage": opt::<String>(&m, "original_language"),
            "contentRating": opt::<String>(&m, "content_rating"),
            "metadata": opt::<serde_json::Value>(&m, "metadata"),
            "scrapedAt": opt::<String>(&m, "scraped_at"),
            "createdAt": opt::<String>(&m, "created_at"),
            "updatedAt": opt::<String>(&m, "updated_at"),
            "genres": genres,
            "credits": credits,
            "files": files,
            "collections": colls,
        })))
    }

    // ── TV Show Detail ──

    pub async fn get_tv_show_detail(db: &impl ConnectionTrait, id: Uuid) -> Result<Option<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT t.id, t.video_id, t.title, t.original_title, t.sort_title, \
             t.year, t.first_air_date::text as first_air_date, t.last_air_date::text as last_air_date, \
             t.status, t.tmdb_rating, t.imdb_rating, t.douban_rating, \
             COALESCE(t.tmdb_rating, t.imdb_rating) as rating, \
             t.tmdb_id, t.imdb_id, t.tvdb_id, t.douban_id, t.bangumi_id, \
             t.poster_path, t.backdrop_path, t.overview, \
             t.is_adult, t.is_favorite, t.original_language, t.content_rating, \
             t.created_at::text as created_at, t.updated_at::text as updated_at \
             FROM tv_shows t WHERE t.id = $1",
            [id.into()],
        );
        let Some(t) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        let genres = Self::query_genres_for_tv(db, id).await?;
        let credits = Self::query_tv_credits(db, id).await?;
        let colls = Self::query_tv_collections(db, id).await?;

        // Seasons
        let season_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT s.id, s.season_number, s.title, s.overview, \
             s.air_date::text as air_date, s.poster_path, s.episode_count \
             FROM seasons s WHERE s.tv_show_id = $1 ORDER BY s.season_number ASC",
            [id.into()],
        );
        let season_rows = db.query_all_raw(season_stmt).await?;

        // Episodes
        let ep_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT e.id, e.season_id, e.episode_number, e.title, e.overview, \
             e.air_date::text as air_date, e.runtime, e.still_path, \
             e.tmdb_rating, e.tmdb_id \
             FROM episodes e WHERE e.tv_show_id = $1 ORDER BY e.episode_number ASC",
            [id.into()],
        );
        let ep_rows = db.query_all_raw(ep_stmt).await?;

        // Group episodes by season
        let mut eps_by_season: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for r in &ep_rows {
            let sid = col::<Uuid>(r, "season_id").map(|v| v.to_string()).unwrap_or_default();
            let sid_clone = sid.clone();
            eps_by_season.entry(sid).or_default().push(json!({
                "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                "seasonId": sid_clone,
                "episodeNumber": col::<i32>(r, "episode_number").unwrap_or(0),
                "title": opt::<String>(r, "title"),
                "overview": opt::<String>(r, "overview"),
                "airDate": opt::<String>(r, "air_date"),
                "runtime": opt::<i32>(r, "runtime"),
                "stillPath": opt::<String>(r, "still_path"),
                "tmdbRating": opt::<f64>(r, "tmdb_rating"),
            }));
        }

        // Files for all episodes via join
        let file_sql = format!(
            "SELECT {FILE_SELECT}, mf.episode_id \
             FROM video_files mf {FILE_JOINS} \
             WHERE mf.episode_id IN (SELECT e.id FROM episodes e WHERE e.tv_show_id = $1) \
             AND mf.is_available = true"
        );
        let file_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &file_sql, [id.into()]);
        let file_rows = db.query_all_raw(file_stmt).await?;

        // Subtitles and chapters for episode files
        let sub_sql = "SELECT s.*, s.file_id FROM subtitles s \
             JOIN video_files mf ON mf.id = s.file_id \
             WHERE mf.episode_id IN (SELECT e.id FROM episodes e WHERE e.tv_show_id = $1) \
             AND mf.is_available = true \
             ORDER BY s.file_id, s.created_at ASC, s.id ASC";
        let sub_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, sub_sql, [id.into()]);
        let sub_rows = db.query_all_raw(sub_stmt).await?;
        let mut subs_map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for r in &sub_rows {
            let fid = col::<Uuid>(r, "file_id").map(|v| v.to_string()).unwrap_or_default();
            subs_map.entry(fid).or_default().push(subtitle_to_json(r));
        }

        let ch_sql = "SELECT c.*, c.file_id FROM chapters c \
             JOIN video_files mf ON mf.id = c.file_id \
             WHERE mf.episode_id IN (SELECT e.id FROM episodes e WHERE e.tv_show_id = $1) \
             AND mf.is_available = true \
             ORDER BY c.file_id, c.index ASC";
        let ch_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, ch_sql, [id.into()]);
        let ch_rows = db.query_all_raw(ch_stmt).await?;
        let mut chs_map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for r in &ch_rows {
            let fid = col::<Uuid>(r, "file_id").map(|v| v.to_string()).unwrap_or_default();
            chs_map.entry(fid).or_default().push(chapter_to_json(r));
        }

        // Group files by episode
        let mut files_by_ep: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for r in &file_rows {
            let eid = col::<Uuid>(r, "episode_id").map(|v| v.to_string()).unwrap_or_default();
            let mut f = file_row_to_json(r)?;
            let fid = f["id"].as_str().unwrap_or("").to_string();
            if let Some(o) = f.as_object_mut() {
                o.insert(
                    "subtitles".into(),
                    json!(subs_map.get(&fid).cloned().unwrap_or_default()),
                );
                o.insert("chapters".into(), json!(chs_map.get(&fid).cloned().unwrap_or_default()));
            }
            files_by_ep.entry(eid).or_default().push(f);
        }

        // Attach episodes (with files) to seasons
        let seasons: Vec<serde_json::Value> = season_rows
            .iter()
            .map(|r| {
                let sid = col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default();
                let mut episodes = eps_by_season.remove(&sid).unwrap_or_default();
                for ep in &mut episodes {
                    let eid = ep["id"].as_str().unwrap_or("").to_string();
                    if let Some(o) = ep.as_object_mut() {
                        o.insert(
                            "files".into(),
                            json!(files_by_ep.get(&eid).cloned().unwrap_or_default()),
                        );
                    }
                }
                json!({
                    "id": sid,
                    "seasonNumber": col::<i32>(r, "season_number").unwrap_or(0),
                    "title": opt::<String>(r, "title"),
                    "overview": opt::<String>(r, "overview"),
                    "airDate": opt::<String>(r, "air_date"),
                    "posterPath": opt::<String>(r, "poster_path"),
                    "episodeCount": opt::<i32>(r, "episode_count"),
                    "episodes": episodes,
                })
            })
            .collect();

        Ok(Some(json!({
            "id": col::<Uuid>(&t, "id")?.to_string(),
            "videoId": col::<Uuid>(&t, "video_id")?.to_string(),
            "title": col::<String>(&t, "title")?,
            "originalTitle": opt::<String>(&t, "original_title"),
            "sortTitle": opt::<String>(&t, "sort_title"),
            "year": opt::<i32>(&t, "year"),
            "firstAirDate": opt::<String>(&t, "first_air_date"),
            "lastAirDate": opt::<String>(&t, "last_air_date"),
            "status": opt::<String>(&t, "status"),
            "rating": opt::<f64>(&t, "rating"),
            "tmdbRating": opt::<f64>(&t, "tmdb_rating"),
            "imdbRating": opt::<f64>(&t, "imdb_rating"),
            "doubanRating": opt::<f64>(&t, "douban_rating"),
            "tmdbId": opt::<String>(&t, "tmdb_id"),
            "imdbId": opt::<String>(&t, "imdb_id"),
            "tvdbId": opt::<String>(&t, "tvdb_id"),
            "doubanId": opt::<String>(&t, "douban_id"),
            "bangumiId": opt::<String>(&t, "bangumi_id"),
            "posterPath": opt::<String>(&t, "poster_path"),
            "backdropPath": opt::<String>(&t, "backdrop_path"),
            "overview": opt::<String>(&t, "overview"),
            "isAdult": col::<bool>(&t, "is_adult").unwrap_or(false),
            "isFavorite": col::<bool>(&t, "is_favorite").unwrap_or(false),
            "originalLanguage": opt::<String>(&t, "original_language"),
            "contentRating": opt::<String>(&t, "content_rating"),
            "createdAt": opt::<String>(&t, "created_at"),
            "updatedAt": opt::<String>(&t, "updated_at"),
            "genres": genres,
            "credits": credits,
            "seasons": seasons,
            "collections": colls,
        })))
    }

    // ── Person Detail ──

    pub async fn get_person_detail(
        db: &impl ConnectionTrait,
        id: Uuid,
        person_type: &str,
    ) -> Result<Option<serde_json::Value>, AppError> {
        // Try the requested table first; if not found, fall back to the other table.
        // This handles the common case where the caller doesn't know which table the
        // person belongs to (e.g., frontend always omits personType).
        let result = Self::query_person_detail_from_table(db, id, person_type).await?;
        if result.is_some() {
            return Ok(result);
        }
        let fallback_type = if person_type == "tv" { "movie" } else { "tv" };
        Self::query_person_detail_from_table(db, id, fallback_type).await
    }

    async fn query_person_detail_from_table(
        db: &impl ConnectionTrait,
        id: Uuid,
        person_type: &str,
    ) -> Result<Option<serde_json::Value>, AppError> {
        let (table, credits_join, credits_fk, extra_id_col) = if person_type == "tv" {
            ("tv_persons", "tv_season_cast", "tv_person_id", "tvdb_id")
        } else {
            ("video_persons", "video_cast", "video_person_id", "imdb_id")
        };

        let sql = format!(
            "SELECT p.id, p.name, p.original_name, p.gender, \
             p.birthday::text as birthday, p.birthplace, p.profile_path, \
             p.profile_key, p.biography, p.deathday::text as deathday, \
             p.known_for_dept, p.popularity, p.tmdb_id, p.{extra_id_col} as extra_id, \
             p.aliases, \
             p.created_at::text as created_at, p.updated_at::text as updated_at \
             FROM {table} p WHERE p.id = $1"
        );
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [id.into()]);
        let Some(p) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        let aliases: Vec<String> = p
            .try_get::<Option<Vec<String>>>("", "aliases")
            .ok()
            .flatten()
            .unwrap_or_default();

        let cred_sql = if person_type == "tv" {
            format!(
                "SELECT tsc.id, tsc.role, tsc.character, tsc.sort_order, \
                 tsc.tv_show_id, NULL::uuid as video_item_id, \
                 t.title as media_title, t.year as media_year, t.poster_path as media_poster, t.video_id \
                 FROM {credits_join} tsc \
                 LEFT JOIN tv_shows t ON t.id = tsc.tv_show_id \
                 WHERE tsc.{credits_fk} = $1 ORDER BY tsc.sort_order ASC"
            )
        } else {
            format!(
                "SELECT mc.id, mc.role, mc.character, mc.sort_order, \
                 mc.video_item_id, NULL::uuid as tv_show_id, \
                 m.title as media_title, m.year as media_year, m.poster_path as media_poster, m.video_id \
                 FROM {credits_join} mc \
                 LEFT JOIN video_items m ON m.id = mc.video_item_id \
                 WHERE mc.{credits_fk} = $1 ORDER BY mc.sort_order ASC"
            )
        };
        let cred_stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &cred_sql, [id.into()]);
        let cred_rows = db.query_all_raw(cred_stmt).await?;
        let credits: Vec<serde_json::Value> = cred_rows
            .iter()
            .map(|r| {
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "role": col::<String>(r, "role").unwrap_or_default(),
                    "character": opt::<String>(r, "character"),
                    "sortOrder": col::<i32>(r, "sort_order").unwrap_or(0),
                    "videoItemId": opt::<Uuid>(r, "video_item_id").map(|v| v.to_string()),
                    "tvShowId": opt::<Uuid>(r, "tv_show_id").map(|v| v.to_string()),
                    "videoId": opt::<Uuid>(r, "video_id").map(|v| v.to_string()),
                    "mediaTitle": opt::<String>(r, "media_title"),
                    "mediaYear": opt::<i32>(r, "media_year"),
                    "mediaPosterPath": opt::<String>(r, "media_poster"),
                })
            })
            .collect();

        Ok(Some(json!({
            "id": col::<Uuid>(&p, "id")?.to_string(),
            "personType": person_type,
            "name": col::<String>(&p, "name")?,
            "originalName": opt::<String>(&p, "original_name"),
            "gender": opt::<String>(&p, "gender"),
            "birthday": opt::<String>(&p, "birthday"),
            "birthplace": opt::<String>(&p, "birthplace"),
            "profilePath": opt::<String>(&p, "profile_path"),
            "profileKey": opt::<String>(&p, "profile_key"),
            "biography": opt::<String>(&p, "biography"),
            "deathday": opt::<String>(&p, "deathday"),
            "knownForDepartment": opt::<String>(&p, "known_for_dept"),
            "popularity": opt::<f64>(&p, "popularity"),
            "tmdbId": opt::<String>(&p, "tmdb_id"),
            "extraId": opt::<String>(&p, "extra_id"),
            "aliases": aliases,
            "createdAt": opt::<String>(&p, "created_at"),
            "updatedAt": opt::<String>(&p, "updated_at"),
            "credits": credits,
        })))
    }

    // ── Extras & Collections ──

    pub async fn list_collections(
        db: &impl ConnectionTrait,
        movie_id: Option<Uuid>,
        tv_show_id: Option<Uuid>,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        if let Some(mid) = movie_id {
            Self::query_video_item_collections(db, mid).await
        } else if let Some(tid) = tv_show_id {
            Self::query_tv_collections(db, tid).await
        } else {
            Ok(vec![])
        }
    }

    // ── Music: Albums ──

    pub async fn list_albums(
        db: &impl ConnectionTrait,
        input: ListAlbumsInput,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let mut conds = vec!["a.music_id = $1".to_string()];
        let mut params: Vec<Value> = vec![input.music_id.into()];
        let mut n = 2usize;

        if let Some(s) = input.search {
            conds.push(format!("a.title ILIKE ${n}"));
            params.push(format!("%{s}%").into());
            n += 1;
        }
        if let Some(g) = input.genre {
            conds.push(format!(
                "EXISTS (SELECT 1 FROM music_tracks mt WHERE mt.album_id = a.id AND mt.genre ILIKE ${n})"
            ));
            params.push(format!("%{g}%").into());
            n += 1;
        }
        if let Some(aid) = input.artist_id {
            conds.push(format!(
                "EXISTS (SELECT 1 FROM music_album_artists maa2 WHERE maa2.album_id = a.id AND maa2.artist_id = ${n})"
            ));
            params.push(aid.into());
            n += 1;
        }
        if let Some(true) = input.favorite {
            conds.push("a.is_favorite = true".to_string());
        }

        let wh = conds.join(" AND ");
        let order = match input.sort_by.as_str() {
            "year" => "a.year",
            "addedAt" | "createdAt" => "a.created_at",
            _ => "a.title",
        };
        let d = dir(&input.sort_dir);

        let total = query_count(
            db,
            &format!("SELECT COUNT(*) as total FROM music_albums a WHERE {wh}"),
            params.clone(),
        )
        .await?;

        let lim = n;
        let off = n + 1;
        let isql = format!(
            "SELECT a.id, a.music_id, a.title, a.sort_title, a.year, \
             a.album_type, a.cover_path, a.is_favorite, a.mb_album_id, \
             a.scraped_at::text as scraped_at, \
             a.metadata->>'genres' as genres_json, \
             a.created_at::text as created_at, a.updated_at::text as updated_at, \
             (SELECT COUNT(*) FROM music_tracks mt WHERE mt.album_id = a.id) as track_count, \
             (SELECT COALESCE(SUM(mt.duration), 0) FROM music_tracks mt WHERE mt.album_id = a.id) as total_duration, \
             (SELECT ma.name FROM music_album_artists maa JOIN music_artists ma ON ma.id = maa.artist_id \
              WHERE maa.album_id = a.id LIMIT 1) as artist_name \
             FROM music_albums a WHERE {wh} ORDER BY {order} {d} NULLS LAST LIMIT ${lim} OFFSET ${off}"
        );
        let offset_val = (input.page - 1) * input.page_size;
        params.push(input.page_size.into());
        params.push(offset_val.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &isql, params);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                let genres: Vec<String> = opt::<String>(r, "genres_json")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                Ok(json!({
                    "id": col::<Uuid>(r, "id")?.to_string(),
                    "musicId": col::<Uuid>(r, "music_id")?.to_string(),
                    "title": col::<String>(r, "title")?,
                    "sortTitle": opt::<String>(r, "sort_title"),
                    "year": opt::<i32>(r, "year"),
                    "albumType": opt::<String>(r, "album_type"),
                    "coverPath": opt::<String>(r, "cover_path"),
                    "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                    "mbAlbumId": opt::<String>(r, "mb_album_id"),
                    "scrapedAt": opt::<String>(r, "scraped_at"),
                    "genres": genres,
                    "trackCount": col::<i64>(r, "track_count").unwrap_or(0),
                    "totalDuration": col::<i64>(r, "total_duration").unwrap_or(0),
                    "artistName": opt::<String>(r, "artist_name"),
                    "createdAt": opt::<String>(r, "created_at"),
                    "updatedAt": opt::<String>(r, "updated_at"),
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok((items, total))
    }

    // ── Music: Album Detail ──

    pub async fn get_album_detail(
        db: &impl ConnectionTrait,
        album_id: Uuid,
    ) -> Result<Option<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT a.id, a.music_id, a.title, a.sort_title, a.year, \
             a.release_date::text as release_date, a.album_type, a.cover_path, \
             a.overview, a.total_tracks, a.total_discs, a.is_favorite, \
             a.mb_album_id, a.metadata, \
             a.scraped_at::text as scraped_at, \
             a.created_at::text as created_at, a.updated_at::text as updated_at \
             FROM music_albums a WHERE a.id = $1",
            [album_id.into()],
        );
        let Some(a) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        let album_cover: Option<String> = opt(&a, "cover_path");
        let album_id_str = col::<Uuid>(&a, "id")?.to_string();

        // Tracks
        let album_title: String = col(&a, "title")?;
        let track_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT t.id, t.title, t.track_number, t.disc_number, t.duration, \
             t.bitrate, t.codec, t.genre, t.sample_rate, t.lyrics_path, \
             mf.id as file_id, mf.path as file_path, mf.filename as file_name, \
             mf.size as file_size, mf.mime_type as file_mime, \
             (SELECT ma.name FROM music_album_artists maa JOIN music_artists ma ON ma.id = maa.artist_id \
              WHERE maa.album_id = t.album_id LIMIT 1) as artist_name \
             FROM music_tracks t \
             LEFT JOIN music_files mf ON mf.track_id = t.id \
             WHERE t.album_id = $1 \
             ORDER BY t.disc_number ASC NULLS FIRST, t.track_number ASC NULLS LAST",
            [album_id.into()],
        );
        let track_rows = db.query_all_raw(track_stmt).await?;
        let tracks: Vec<serde_json::Value> = track_rows
            .iter()
            .map(|r| {
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "albumId": &album_id_str,
                    "albumTitle": &album_title,
                    "title": col::<String>(r, "title").unwrap_or_default(),
                    "artistName": opt::<String>(r, "artist_name"),
                    "trackNumber": opt::<i32>(r, "track_number"),
                    "discNumber": opt::<i32>(r, "disc_number"),
                    "duration": opt::<i32>(r, "duration"),
                    "bitrate": opt::<i32>(r, "bitrate"),
                    "codec": opt::<String>(r, "codec"),
                    "genre": opt::<String>(r, "genre"),
                    "sampleRate": opt::<i32>(r, "sample_rate"),
                    "lyricsPath": opt::<String>(r, "lyrics_path"),
                    "coverPath": &album_cover,
                    "fileId": opt::<Uuid>(r, "file_id").map(|v| v.to_string()),
                    "file": opt::<Uuid>(r, "file_id").map(|fid| json!({
                        "id": fid.to_string(),
                        "path": opt::<String>(r, "file_path"),
                        "filename": opt::<String>(r, "file_name"),
                        "size": opt::<i64>(r, "file_size"),
                        "mimeType": opt::<String>(r, "file_mime"),
                    })),
                })
            })
            .collect();

        // Credits
        let credits = Self::query_album_credits(db, album_id).await?;

        // Parse genres from metadata
        let metadata: Option<serde_json::Value> = opt(&a, "metadata");
        let genres: Vec<String> = metadata
            .as_ref()
            .and_then(|m| m.get("genres"))
            .and_then(|g| g.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        Ok(Some(json!({
            "id": &album_id_str,
            "musicId": col::<Uuid>(&a, "music_id")?.to_string(),
            "title": col::<String>(&a, "title")?,
            "sortTitle": opt::<String>(&a, "sort_title"),
            "year": opt::<i32>(&a, "year"),
            "releaseDate": opt::<String>(&a, "release_date"),
            "albumType": opt::<String>(&a, "album_type"),
            "coverPath": &album_cover,
            "overview": opt::<String>(&a, "overview"),
            "totalTracks": opt::<i32>(&a, "total_tracks"),
            "totalDiscs": opt::<i32>(&a, "total_discs"),
            "isFavorite": col::<bool>(&a, "is_favorite").unwrap_or(false),
            "mbAlbumId": opt::<String>(&a, "mb_album_id"),
            "genres": genres,
            "scrapedAt": opt::<String>(&a, "scraped_at"),
            "metadata": metadata,
            "createdAt": opt::<String>(&a, "created_at"),
            "updatedAt": opt::<String>(&a, "updated_at"),
            "tracks": tracks,
            "credits": credits,
        })))
    }

    // ── Music: Tracks ──

    pub async fn list_tracks(
        db: &impl ConnectionTrait,
        input: ListTracksInput,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let mut conds = vec!["a.music_id = $1".to_string()];
        let mut params: Vec<Value> = vec![input.music_id.into()];
        let mut n = 2usize;

        if let Some(s) = input.search {
            conds.push(format!("t.title ILIKE ${n}"));
            params.push(format!("%{s}%").into());
            n += 1;
        }
        if let Some(g) = input.genre {
            conds.push(format!("t.genre ILIKE ${n}"));
            params.push(format!("%{g}%").into());
            n += 1;
        }

        let wh = conds.join(" AND ");
        let order = match input.sort_by.as_str() {
            "duration" => "t.duration",
            "addedAt" | "createdAt" => "a.created_at",
            _ => "t.title",
        };
        let d = dir(&input.sort_dir);

        let total = query_count(
            db,
            &format!(
                "SELECT COUNT(*) as total FROM music_tracks t \
                 JOIN music_albums a ON a.id = t.album_id WHERE {wh}"
            ),
            params.clone(),
        )
        .await?;

        let lim = n;
        let off = n + 1;
        let isql = format!(
            "SELECT t.id, t.title, t.track_number, t.disc_number, t.duration, \
             t.bitrate, t.codec, t.genre, t.sample_rate, \
             a.title as album_title, a.cover_path as album_cover, \
             mf.id as file_id, mf.path as file_path, mf.filename as file_name, \
             mf.size as file_size, mf.mime_type as file_mime, \
             (SELECT ma.name FROM music_album_artists maa JOIN music_artists ma ON ma.id = maa.artist_id \
              WHERE maa.album_id = a.id LIMIT 1) as artist_name \
             FROM music_tracks t \
             JOIN music_albums a ON a.id = t.album_id \
             LEFT JOIN music_files mf ON mf.track_id = t.id \
             WHERE {wh} ORDER BY {order} {d} NULLS LAST LIMIT ${lim} OFFSET ${off}"
        );
        let offset_val = (input.page - 1) * input.page_size;
        params.push(input.page_size.into());
        params.push(offset_val.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &isql, params);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(json!({
                    "id": col::<Uuid>(r, "id")?.to_string(),
                    "title": col::<String>(r, "title")?,
                    "trackNumber": opt::<i32>(r, "track_number"),
                    "discNumber": opt::<i32>(r, "disc_number"),
                    "duration": opt::<i32>(r, "duration"),
                    "bitrate": opt::<i32>(r, "bitrate"),
                    "codec": opt::<String>(r, "codec"),
                    "genre": opt::<String>(r, "genre"),
                    "sampleRate": opt::<i32>(r, "sample_rate"),
                    "albumTitle": opt::<String>(r, "album_title"),
                    "albumCover": opt::<String>(r, "album_cover"),
                    "coverPath": opt::<String>(r, "album_cover"),
                    "artistName": opt::<String>(r, "artist_name"),
                    "fileId": opt::<Uuid>(r, "file_id").map(|v| v.to_string()),
                    "file": opt::<Uuid>(r, "file_id").map(|fid| json!({
                        "id": fid.to_string(),
                        "path": opt::<String>(r, "file_path"),
                        "filename": opt::<String>(r, "file_name"),
                        "size": opt::<i64>(r, "file_size"),
                        "mimeType": opt::<String>(r, "file_mime"),
                    })),
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok((items, total))
    }

    // ── Music: Artists ──

    pub async fn list_artists(
        db: &impl ConnectionTrait,
        music_id: Uuid,
        page: i64,
        page_size: i64,
        sort_by: &str,
        sort_dir: &str,
        search: Option<&str>,
    ) -> Result<(Vec<serde_json::Value>, i64), AppError> {
        let mut conds = vec!["a.music_id = $1".to_string()];
        let mut params: Vec<Value> = vec![music_id.into()];
        let mut n = 2usize;

        if let Some(s) = search {
            conds.push(format!("ma.name ILIKE ${n}"));
            params.push(format!("%{s}%").into());
            n += 1;
        }

        let wh = conds.join(" AND ");
        let order = match sort_by {
            "albumCount" => "album_count",
            "addedAt" | "createdAt" => "ma.created_at",
            _ => "ma.name",
        };
        let d = dir(sort_dir);

        let count_sql = format!(
            "SELECT COUNT(DISTINCT ma.id) as total FROM music_artists ma \
             JOIN music_album_artists maa ON maa.artist_id = ma.id \
             JOIN music_albums a ON a.id = maa.album_id WHERE {wh}"
        );
        let total = query_count(db, &count_sql, params.clone()).await?;

        let lim = n;
        let off = n + 1;
        let isql = format!(
            "SELECT ma.id, ma.name, ma.profile_path, \
             COUNT(DISTINCT maa.album_id) as album_count \
             FROM music_artists ma \
             JOIN music_album_artists maa ON maa.artist_id = ma.id \
             JOIN music_albums a ON a.id = maa.album_id \
             WHERE {wh} \
             GROUP BY ma.id \
             ORDER BY {order} {d} NULLS LAST LIMIT ${lim} OFFSET ${off}"
        );
        let offset_val = (page - 1) * page_size;
        params.push(page_size.into());
        params.push(offset_val.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &isql, params);
        let rows = db.query_all_raw(stmt).await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(json!({
                    "id": col::<Uuid>(r, "id")?.to_string(),
                    "name": col::<String>(r, "name")?,
                    "profilePath": opt::<String>(r, "profile_path"),
                    "albumCount": col::<i64>(r, "album_count").unwrap_or(0),
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok((items, total))
    }

    // ── Music: Artist Detail ──

    pub async fn get_artist_detail(
        db: &impl ConnectionTrait,
        person_id: Uuid,
        music_id: Uuid,
    ) -> Result<Option<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT ma.id, ma.name, ma.original_name, ma.profile_path, ma.biography, \
             ma.popularity, ma.followers, ma.mb_id, \
             (SELECT COUNT(*) FROM music_album_artists maa2 \
              JOIN music_albums a2 ON a2.id = maa2.album_id AND a2.music_id = $2 \
              WHERE maa2.artist_id = ma.id) as album_count, \
             (SELECT COUNT(*) FROM music_album_artists maa3 \
              JOIN music_tracks t3 ON t3.album_id = maa3.album_id \
              JOIN music_albums a3 ON a3.id = maa3.album_id AND a3.music_id = $2 \
              WHERE maa3.artist_id = ma.id) as track_count \
             FROM music_artists ma WHERE ma.id = $1",
            [person_id.into(), music_id.into()],
        );
        let Some(p) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        let album_stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT a.id, a.title, a.year, a.cover_path, a.is_favorite, a.album_type \
             FROM music_albums a \
             JOIN music_album_artists maa ON maa.album_id = a.id AND maa.artist_id = $1 \
             WHERE a.music_id = $2 \
             ORDER BY a.year DESC NULLS LAST",
            [person_id.into(), music_id.into()],
        );
        let album_rows = db.query_all_raw(album_stmt).await?;
        let albums: Vec<serde_json::Value> = album_rows
            .iter()
            .map(|r| {
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "title": col::<String>(r, "title").unwrap_or_default(),
                    "year": opt::<i32>(r, "year"),
                    "coverPath": opt::<String>(r, "cover_path"),
                    "isFavorite": col::<bool>(r, "is_favorite").unwrap_or(false),
                    "albumType": opt::<String>(r, "album_type"),
                })
            })
            .collect();

        Ok(Some(json!({
            "id": col::<Uuid>(&p, "id")?.to_string(),
            "name": col::<String>(&p, "name")?,
            "originalName": opt::<String>(&p, "original_name"),
            "profilePath": opt::<String>(&p, "profile_path"),
            "biography": opt::<String>(&p, "biography"),
            "popularity": opt::<i32>(&p, "popularity"),
            "followers": opt::<i32>(&p, "followers"),
            "mbArtistId": opt::<String>(&p, "mb_id"),
            "albumCount": col::<i64>(&p, "album_count").unwrap_or(0),
            "trackCount": col::<i64>(&p, "track_count").unwrap_or(0),
            "albums": albums,
        })))
    }

    // ── Music: Toggle Album Favorite ──

    pub async fn toggle_album_favorite(db: &impl ConnectionTrait, album_id: Uuid) -> Result<bool, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE music_albums SET is_favorite = NOT is_favorite WHERE id = $1 RETURNING is_favorite",
            [album_id.into()],
        );
        let row = db.query_one_raw(stmt).await?.not_found("album not found")?;
        col(&row, "is_favorite")
    }

    // ── Track Lyrics ──

    pub async fn get_track_lyrics(db: &impl ConnectionTrait, track_id: Uuid) -> Result<Option<String>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT t.lyrics_path FROM music_tracks t WHERE t.id = $1",
            [track_id.into()],
        );
        let row = db.query_one_raw(stmt).await?.not_found("track not found")?;
        Ok(opt(&row, "lyrics_path"))
    }

    // ── Play URL ──

    pub async fn get_play_url(db: &impl ConnectionTrait, media_file_id: Uuid) -> Result<Option<String>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT mf.id FROM video_files mf WHERE mf.id = $1",
            [media_file_id.into()],
        );
        let Some(_row) = db.query_one_raw(stmt).await? else {
            return Ok(None);
        };

        // Media server-based play URLs are no longer supported.
        Ok(None)
    }

    // ── Private helpers ──

    async fn query_genres_for_video_item(
        db: &impl ConnectionTrait,
        movie_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"SELECT g.id, g.tmdb_genre_id FROM genres g
               JOIN video_genres mg ON mg.genre_id = g.id WHERE mg.video_item_id = $1",
            [movie_id.into()],
        );
        let rows = db.query_all_raw(stmt).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let tmdb_id = col::<i32>(r, "tmdb_genre_id").unwrap_or(0);
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "tmdbGenreId": tmdb_id,
                    "name": tmdb_genre_name(tmdb_id),
                })
            })
            .collect())
    }

    async fn query_genres_for_tv(db: &impl ConnectionTrait, tv_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"SELECT g.id, g.tmdb_genre_id FROM genres g
               JOIN tv_show_genres tg ON tg.genre_id = g.id WHERE tg.tv_show_id = $1",
            [tv_id.into()],
        );
        let rows = db.query_all_raw(stmt).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let tmdb_id = col::<i32>(r, "tmdb_genre_id").unwrap_or(0);
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "tmdbGenreId": tmdb_id,
                    "name": tmdb_genre_name(tmdb_id),
                })
            })
            .collect())
    }

    async fn query_video_item_credits(
        db: &impl ConnectionTrait,
        movie_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT c.id, c.role, c.character, c.sort_order, \
             p.id as person_id, p.name, p.profile_path \
             FROM video_cast c JOIN video_persons p ON p.id = c.video_person_id \
             WHERE c.video_item_id = $1 ORDER BY c.sort_order ASC",
            [movie_id.into()],
        );
        Ok(Self::map_credit_rows(db.query_all_raw(stmt).await?))
    }

    async fn query_tv_credits(db: &impl ConnectionTrait, tv_show_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT DISTINCT ON (c.tv_person_id, c.role) \
             c.id, c.role, c.character, c.sort_order, \
             p.id as person_id, p.name, p.profile_path \
             FROM tv_season_cast c JOIN tv_persons p ON p.id = c.tv_person_id \
             WHERE c.tv_show_id = $1 ORDER BY c.tv_person_id, c.role, c.sort_order ASC",
            [tv_show_id.into()],
        );
        Ok(Self::map_credit_rows(db.query_all_raw(stmt).await?))
    }

    async fn query_album_credits(db: &impl ConnectionTrait, album_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT aa.id, aa.role, NULL::text as character, aa.sort_order, \
             ma.id as person_id, ma.name, ma.profile_path \
             FROM music_album_artists aa JOIN music_artists ma ON ma.id = aa.artist_id \
             WHERE aa.album_id = $1 ORDER BY aa.sort_order ASC",
            [album_id.into()],
        );
        Ok(Self::map_credit_rows(db.query_all_raw(stmt).await?))
    }

    fn map_credit_rows(rows: Vec<sea_orm::QueryResult>) -> Vec<serde_json::Value> {
        rows.iter()
            .map(|r| {
                json!({
                    "id": col::<Uuid>(r, "id").map(|v| v.to_string()).unwrap_or_default(),
                    "role": col::<String>(r, "role").unwrap_or_default(),
                    "character": opt::<String>(r, "character"),
                    "sortOrder": col::<i32>(r, "sort_order").unwrap_or(0),
                    "person": {
                        "id": col::<Uuid>(r, "person_id").map(|v| v.to_string()).unwrap_or_default(),
                        "name": col::<String>(r, "name").unwrap_or_default(),
                        "profilePath": opt::<String>(r, "profile_path"),
                    }
                })
            })
            .collect()
    }

    async fn query_files_with_nested(
        db: &impl ConnectionTrait,
        fk_col: &str,
        parent_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let sql = format!(
            "SELECT {FILE_SELECT} FROM video_files mf {FILE_JOINS} \
             WHERE mf.{fk_col} = $1 AND mf.is_available = true"
        );
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Postgres, &sql, [parent_id.into()]);
        let file_rows = db.query_all_raw(stmt).await?;

        let subs_map = query_subtitles_for_files(db, fk_col, parent_id).await?;
        let chs_map = query_chapters_for_files(db, fk_col, parent_id).await?;

        let mut files = Vec::with_capacity(file_rows.len());
        for r in &file_rows {
            let mut f = file_row_to_json(r)?;
            let fid = f["id"].as_str().unwrap_or("").to_string();
            if let Some(obj) = f.as_object_mut() {
                obj.insert(
                    "subtitles".into(),
                    json!(subs_map.get(&fid).cloned().unwrap_or_default()),
                );
                obj.insert("chapters".into(), json!(chs_map.get(&fid).cloned().unwrap_or_default()));
            }
            files.push(f);
        }
        Ok(files)
    }

    async fn query_video_item_collections(
        db: &impl ConnectionTrait,
        movie_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"SELECT c.id, c.name, c.poster_path, c.overview FROM collections c
               JOIN video_collections mc ON mc.collection_id = c.id WHERE mc.video_item_id = $1",
            [movie_id.into()],
        );
        let rows = db.query_all_raw(stmt).await?;
        Ok(rows.iter().map(collection_to_json).collect())
    }

    async fn query_tv_collections(db: &impl ConnectionTrait, tv_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"SELECT c.id, c.name, c.poster_path, c.overview FROM collections c
               JOIN tv_show_collections tc ON tc.collection_id = c.id WHERE tc.tv_show_id = $1",
            [tv_id.into()],
        );
        let rows = db.query_all_raw(stmt).await?;
        Ok(rows.iter().map(collection_to_json).collect())
    }
}
