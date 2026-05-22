pub mod datetime;
pub mod entities;
pub mod models;
pub mod pagination;
pub mod repos;

pub use datetime::{ApiDateTimeExt, OptionalApiDateTimeExt};

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

const SCHEMA: &str = "video";

pub async fn init_pool() -> anyhow::Result<DatabaseConnection> {
    let base_url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let sep = if base_url.contains('?') { '&' } else { '?' };
    // Set search_path so raw SQL queries (heavy use in media_content_repo etc.)
    // resolve unqualified table names against the video schema first.
    let url = format!(
        "{base_url}{sep}application_name=tokimo-app-video&options=-c%20search_path%3Dvideo,public"
    );
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(20).min_connections(2).sqlx_logging(false);
    Ok(Database::connect(opt).await?)
}

pub async fn init_schema(db: &DatabaseConnection) -> anyhow::Result<()> {
    let ddl = vec![
        // schema
        format!(r#"CREATE SCHEMA IF NOT EXISTS "{SCHEMA}""#),

        // users
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ,
    otp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    otp_secret TEXT
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS users_username_idx ON "{SCHEMA}".users (username)"#),

        // sessions
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    created_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    user_agent TEXT,
    browser TEXT,
    browser_version TEXT,
    os TEXT
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON "{SCHEMA}".sessions (user_id)"#),

        // user_preferences
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".user_preferences (
    user_id UUID NOT NULL,
    scope TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, scope, scope_id)
)"#),

        // system_config
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".system_config (
    scope TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (scope, scope_id)
)"#),

        // vfs
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".vfs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    config JSONB,
    sort_order INTEGER NOT NULL DEFAULT 0,
    last_scan_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // file_favorites
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".file_favorites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    vfs_id UUID NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    is_directory BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS file_favorites_user_id_vfs_id_path_key ON "{SCHEMA}".file_favorites (user_id, vfs_id, path)"#),

        // jobs
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    user_id UUID,
    parent_job_id UUID,
    task_type TEXT,
    payload JSONB NOT NULL DEFAULT '{{}}',
    meta JSONB,
    progress INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    error TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dedupe_key TEXT,
    alias_job_id UUID,
    priority INTEGER NOT NULL DEFAULT 0
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS jobs_status_idx ON "{SCHEMA}".jobs (status)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS jobs_type_idx ON "{SCHEMA}".jobs (type)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS jobs_parent_job_id_idx ON "{SCHEMA}".jobs (parent_job_id)"#),

        // collections
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".collections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sort_title TEXT,
    overview TEXT,
    poster_path TEXT,
    backdrop_path TEXT,
    tmdb_collection_id TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS collections_tmdb_collection_id_key ON "{SCHEMA}".collections (tmdb_collection_id) WHERE tmdb_collection_id IS NOT NULL"#),

        // genres
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".genres (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tmdb_genre_id INTEGER NOT NULL
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS genres_tmdb_genre_id_key ON "{SCHEMA}".genres (tmdb_genre_id)"#),

        // videos
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".videos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    avatar JSONB,
    description TEXT,
    poster_path TEXT,
    scrape_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    scrape_agents TEXT[],
    sort_order INTEGER NOT NULL DEFAULT 0,
    settings JSONB,
    sources JSONB NOT NULL DEFAULT '[]',
    sync_status TEXT NOT NULL DEFAULT 'idle',
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // books
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".books (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    avatar JSONB,
    description TEXT,
    poster_path TEXT,
    scrape_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    scrape_agents TEXT[],
    sort_order INTEGER NOT NULL DEFAULT 0,
    settings JSONB,
    sources JSONB NOT NULL DEFAULT '[]',
    sync_status TEXT NOT NULL DEFAULT 'idle',
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // musics
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".musics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    avatar JSONB,
    description TEXT,
    poster_path TEXT,
    scrape_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    scrape_agents TEXT[],
    sort_order INTEGER NOT NULL DEFAULT 0,
    settings JSONB,
    sources JSONB NOT NULL DEFAULT '[]',
    sync_status TEXT NOT NULL DEFAULT 'idle',
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // pt_sites
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".pt_sites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    site_id TEXT NOT NULL,
    domain TEXT NOT NULL,
    auth_type TEXT NOT NULL DEFAULT 'cookies',
    cookies TEXT,
    api_key TEXT,
    config_yaml TEXT,
    config_url TEXT,
    auto_stop_minutes TEXT,
    traffic_manage_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    traffic_manage_mode TEXT NOT NULL DEFAULT 'active',
    traffic_manage_target TEXT,
    adult_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    last_checked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // download_clients
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".download_clients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    url TEXT NOT NULL,
    username TEXT,
    password TEXT,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    require_auth BOOLEAN NOT NULL DEFAULT FALSE,
    monitor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    poll_interval TEXT NOT NULL DEFAULT '60s',
    download_path TEXT NOT NULL DEFAULT '',
    mapped_path TEXT,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // download_records
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".download_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    torrent_hash TEXT,
    app_id TEXT NOT NULL,
    downloader_type TEXT NOT NULL,
    source_site TEXT,
    source_url TEXT,
    app_metadata JSONB,
    download_client_id UUID,
    download_path TEXT,
    target_path TEXT,
    file_size TEXT,
    thumbnail_url TEXT,
    download_speed BIGINT,
    eta_seconds INTEGER,
    downloaded_bytes BIGINT,
    error_message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    progress DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS download_records_status_idx ON "{SCHEMA}".download_records (status)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS download_records_app_id_idx ON "{SCHEMA}".download_records (app_id)"#),

        // subscription_filters
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".subscription_filters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sources JSONB,
    resolutions JSONB,
    codecs JSONB,
    release_groups JSONB,
    min_size TEXT NOT NULL DEFAULT '0',
    max_size TEXT NOT NULL DEFAULT '0',
    min_seeders TEXT NOT NULL DEFAULT '0',
    max_seeders TEXT NOT NULL DEFAULT '0',
    include_keywords TEXT,
    exclude_keywords TEXT,
    free_only BOOLEAN NOT NULL DEFAULT FALSE,
    exclude_hr BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_by UUID,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),

        // subscriptions
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_mode TEXT NOT NULL,
    media_type TEXT NOT NULL,
    tmdb_id TEXT,
    title TEXT NOT NULL,
    year TEXT,
    poster_path TEXT,
    season TEXT,
    episodes JSONB,
    series_prefix TEXT,
    metadata_source TEXT,
    max_downloads_per_run INTEGER NOT NULL DEFAULT 3,
    filter_id UUID,
    filter_ids JSONB,
    filter_overrides JSONB,
    status TEXT NOT NULL DEFAULT 'active',
    interval_minutes TEXT NOT NULL DEFAULT '60',
    site_ids JSONB,
    download_client_id UUID,
    target_video_id UUID,
    last_checked_at TIMESTAMPTZ,
    next_check_at TIMESTAMPTZ,
    created_by UUID,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS subscriptions_status_idx ON "{SCHEMA}".subscriptions (status)"#),

        // organize_reports
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".organize_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_path TEXT NOT NULL,
    total_items TEXT NOT NULL,
    success_count TEXT NOT NULL,
    failed_count TEXT NOT NULL,
    skipped_count TEXT NOT NULL,
    results JSONB NOT NULL DEFAULT '[]',
    media_names JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ
)"#),

        // scrape_settings
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".scrape_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    settings_json JSONB NOT NULL DEFAULT '{{}}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),

        // scrape_tasks
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".scrape_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type TEXT NOT NULL,
    target_id UUID NOT NULL,
    agent TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    error_msg TEXT,
    retries INTEGER NOT NULL DEFAULT 0,
    scheduled_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS scrape_tasks_status_idx ON "{SCHEMA}".scrape_tasks (status)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS scrape_tasks_target_idx ON "{SCHEMA}".scrape_tasks (target_type, target_id)"#),

        // video_persons
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_persons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    original_name TEXT,
    aliases TEXT[],
    gender TEXT,
    birthday DATE,
    birthplace TEXT,
    profile_path TEXT,
    profile_key TEXT,
    biography TEXT,
    deathday DATE,
    known_for_dept TEXT,
    popularity DOUBLE PRECISION,
    tmdb_id TEXT,
    imdb_id TEXT,
    javbus_id TEXT,
    javdb_id TEXT,
    tpdb_id TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_persons_tmdb_id_key ON "{SCHEMA}".video_persons (tmdb_id) WHERE tmdb_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_persons_imdb_id_key ON "{SCHEMA}".video_persons (imdb_id) WHERE imdb_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_persons_javbus_id_key ON "{SCHEMA}".video_persons (javbus_id) WHERE javbus_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_persons_javdb_id_key ON "{SCHEMA}".video_persons (javdb_id) WHERE javdb_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_persons_tpdb_id_key ON "{SCHEMA}".video_persons (tpdb_id) WHERE tpdb_id IS NOT NULL"#),

        // tv_persons
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".tv_persons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    original_name TEXT,
    aliases TEXT[],
    gender TEXT,
    birthday DATE,
    birthplace TEXT,
    profile_path TEXT,
    profile_key TEXT,
    biography TEXT,
    deathday DATE,
    known_for_dept TEXT,
    popularity DOUBLE PRECISION,
    tmdb_id TEXT,
    tvdb_id TEXT,
    imdb_id TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_persons_tmdb_id_key ON "{SCHEMA}".tv_persons (tmdb_id) WHERE tmdb_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_persons_tvdb_id_key ON "{SCHEMA}".tv_persons (tvdb_id) WHERE tvdb_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_persons_imdb_id_key ON "{SCHEMA}".tv_persons (imdb_id) WHERE imdb_id IS NOT NULL"#),

        // tv_shows
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".tv_shows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    video_id UUID NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    first_air_date DATE,
    last_air_date DATE,
    status TEXT,
    tmdb_rating DOUBLE PRECISION,
    imdb_rating DOUBLE PRECISION,
    douban_rating DOUBLE PRECISION,
    tmdb_id TEXT,
    imdb_id TEXT,
    tvdb_id TEXT,
    douban_id TEXT,
    bangumi_id TEXT,
    poster_path TEXT,
    backdrop_path TEXT,
    overview TEXT,
    is_adult BOOLEAN NOT NULL DEFAULT FALSE,
    is_favorite BOOLEAN NOT NULL DEFAULT FALSE,
    original_language TEXT,
    countries TEXT[],
    content_rating TEXT,
    content_advisories TEXT[],
    locked_fields TEXT[],
    metadata JSONB,
    scraped_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_shows_video_id_tvdb_id_key ON "{SCHEMA}".tv_shows (video_id, tvdb_id)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_shows_video_id_tmdb_id_key ON "{SCHEMA}".tv_shows (video_id, tmdb_id)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_shows_video_id_imdb_id_key ON "{SCHEMA}".tv_shows (video_id, imdb_id)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_shows_douban_id_key ON "{SCHEMA}".tv_shows (douban_id) WHERE douban_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_shows_bangumi_id_key ON "{SCHEMA}".tv_shows (bangumi_id) WHERE bangumi_id IS NOT NULL"#),
        format!(r#"CREATE INDEX IF NOT EXISTS tv_shows_video_id_idx ON "{SCHEMA}".tv_shows (video_id)"#),

        // tv_show_collections
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".tv_show_collections (
    tv_show_id UUID NOT NULL,
    collection_id UUID NOT NULL,
    PRIMARY KEY (tv_show_id, collection_id)
)"#),

        // tv_show_genres
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".tv_show_genres (
    tv_show_id UUID NOT NULL,
    genre_id UUID NOT NULL,
    PRIMARY KEY (tv_show_id, genre_id)
)"#),

        // seasons
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tv_show_id UUID NOT NULL,
    season_number INTEGER NOT NULL,
    title TEXT,
    overview TEXT,
    air_date DATE,
    poster_path TEXT,
    episode_count INTEGER
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS seasons_tv_show_id_season_number_key ON "{SCHEMA}".seasons (tv_show_id, season_number)"#),

        // tv_season_cast
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".tv_season_cast (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tv_show_id UUID NOT NULL,
    season_id UUID NOT NULL,
    tv_person_id UUID NOT NULL,
    role TEXT NOT NULL,
    character TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS tv_season_cast_season_id_tv_person_id_role_key ON "{SCHEMA}".tv_season_cast (season_id, tv_person_id, role)"#),

        // episodes
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".episodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tv_show_id UUID NOT NULL,
    season_id UUID NOT NULL,
    episode_number INTEGER NOT NULL,
    title TEXT,
    overview TEXT,
    air_date DATE,
    runtime INTEGER,
    still_path TEXT,
    tmdb_rating DOUBLE PRECISION,
    tmdb_id TEXT
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS episodes_season_id_episode_number_key ON "{SCHEMA}".episodes (season_id, episode_number)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS episodes_tmdb_id_key ON "{SCHEMA}".episodes (tmdb_id) WHERE tmdb_id IS NOT NULL"#),

        // video_items
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    video_id UUID NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date DATE,
    runtime INTEGER,
    tmdb_rating DOUBLE PRECISION,
    imdb_rating DOUBLE PRECISION,
    douban_rating DOUBLE PRECISION,
    tmdb_id TEXT,
    imdb_id TEXT,
    douban_id TEXT,
    jav_number TEXT,
    javbus_id TEXT,
    javdb_id TEXT,
    poster_path TEXT,
    backdrop_path TEXT,
    overview TEXT,
    tagline TEXT,
    is_adult BOOLEAN NOT NULL DEFAULT FALSE,
    is_favorite BOOLEAN NOT NULL DEFAULT FALSE,
    original_language TEXT,
    countries TEXT[],
    spoken_languages TEXT[],
    content_rating TEXT,
    content_advisories TEXT[],
    locked_fields TEXT[],
    metadata JSONB,
    scraped_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_video_id_imdb_id_key ON "{SCHEMA}".video_items (video_id, imdb_id)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_video_id_tmdb_id_key ON "{SCHEMA}".video_items (video_id, tmdb_id)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_douban_id_key ON "{SCHEMA}".video_items (douban_id) WHERE douban_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_jav_number_key ON "{SCHEMA}".video_items (jav_number) WHERE jav_number IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_javbus_id_key ON "{SCHEMA}".video_items (javbus_id) WHERE javbus_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_items_javdb_id_key ON "{SCHEMA}".video_items (javdb_id) WHERE javdb_id IS NOT NULL"#),
        format!(r#"CREATE INDEX IF NOT EXISTS video_items_video_id_idx ON "{SCHEMA}".video_items (video_id)"#),

        // video_cast
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_cast (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    video_item_id UUID NOT NULL,
    video_person_id UUID NOT NULL,
    role TEXT NOT NULL,
    character TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_cast_video_item_id_video_person_id_role_key ON "{SCHEMA}".video_cast (video_item_id, video_person_id, role)"#),

        // video_collections
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_collections (
    video_item_id UUID NOT NULL,
    collection_id UUID NOT NULL,
    PRIMARY KEY (video_item_id, collection_id)
)"#),

        // video_genres
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_genres (
    video_item_id UUID NOT NULL,
    genre_id UUID NOT NULL,
    PRIMARY KEY (video_item_id, genre_id)
)"#),

        // video_files
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".video_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID,
    path TEXT NOT NULL,
    filename TEXT NOT NULL,
    size BIGINT,
    mime_type TEXT,
    duration INTEGER,
    checksum TEXT,
    video_codec TEXT,
    video_width INTEGER,
    video_height INTEGER,
    video_profile TEXT,
    hdr_type TEXT,
    video_streams JSONB,
    audio_streams JSONB,
    ffprobe_raw JSONB,
    iso_meta JSONB,
    is_available BOOLEAN NOT NULL DEFAULT TRUE,
    scanned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    video_item_id UUID,
    episode_id UUID
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS video_files_source_id_path_key ON "{SCHEMA}".video_files (source_id, path)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS video_files_video_item_id_idx ON "{SCHEMA}".video_files (video_item_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS video_files_episode_id_idx ON "{SCHEMA}".video_files (episode_id)"#),

        // chapters
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".chapters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL,
    index INTEGER NOT NULL,
    title TEXT,
    start_time INTEGER NOT NULL DEFAULT 0,
    end_time INTEGER,
    thumb_path TEXT
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS chapters_file_id_index_key ON "{SCHEMA}".chapters (file_id, index)"#),

        // subtitles
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".subtitles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL,
    language TEXT NOT NULL,
    title TEXT,
    source_type TEXT NOT NULL,
    format TEXT NOT NULL,
    path TEXT,
    s3_key TEXT,
    source TEXT,
    source_id TEXT,
    encoding TEXT,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    is_forced BOOLEAN NOT NULL DEFAULT FALSE,
    is_hearing_impaired BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS subtitles_file_id_idx ON "{SCHEMA}".subtitles (file_id)"#),

        // media_arts
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".media_arts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    video_item_id UUID,
    tv_show_id UUID,
    season_id UUID,
    album_id UUID,
    book_id UUID,
    art_type TEXT NOT NULL,
    url TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    language TEXT,
    source TEXT,
    is_selected BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS media_arts_video_item_id_idx ON "{SCHEMA}".media_arts (video_item_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS media_arts_tv_show_id_idx ON "{SCHEMA}".media_arts (tv_show_id)"#),

        // watch_histories
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".watch_histories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    file_id UUID NOT NULL,
    session_id UUID,
    client_name TEXT,
    user_agent TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stopped_at TIMESTAMPTZ,
    position INTEGER NOT NULL DEFAULT 0,
    duration INTEGER,
    completed BOOLEAN NOT NULL DEFAULT FALSE
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS watch_histories_user_id_idx ON "{SCHEMA}".watch_histories (user_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS watch_histories_file_id_idx ON "{SCHEMA}".watch_histories (file_id)"#),

        // playback_sessions
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".playback_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    session_id UUID,
    file_id UUID NOT NULL,
    client_name TEXT,
    user_agent TEXT,
    play_method TEXT NOT NULL,
    source_container TEXT,
    source_video_codec TEXT,
    source_video_profile TEXT,
    source_hdr_type TEXT,
    source_width INTEGER,
    source_height INTEGER,
    source_duration INTEGER,
    source_file_size BIGINT,
    transcode_video_codec TEXT,
    transcode_audio_codec TEXT,
    transcode_reasons JSONB,
    media_streams_raw JSONB,
    client_capabilities JSONB,
    position INTEGER NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stopped_at TIMESTAMPTZ
)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS playback_sessions_user_id_idx ON "{SCHEMA}".playback_sessions (user_id)"#),
        format!(r#"CREATE INDEX IF NOT EXISTS playback_sessions_file_id_idx ON "{SCHEMA}".playback_sessions (file_id)"#),

        // user_media_states
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".user_media_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    video_item_id UUID,
    tv_show_id UUID,
    episode_id UUID,
    book_id UUID,
    chapter_id UUID,
    resume_position INTEGER NOT NULL DEFAULT 0,
    play_count INTEGER NOT NULL DEFAULT 0,
    is_watched BOOLEAN NOT NULL DEFAULT FALSE,
    last_watch_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_states_user_id_video_item_id_key ON "{SCHEMA}".user_media_states (user_id, video_item_id) WHERE video_item_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_states_user_id_tv_show_id_key ON "{SCHEMA}".user_media_states (user_id, tv_show_id) WHERE tv_show_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_states_user_id_episode_id_key ON "{SCHEMA}".user_media_states (user_id, episode_id) WHERE episode_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_states_user_id_book_id_key ON "{SCHEMA}".user_media_states (user_id, book_id) WHERE book_id IS NOT NULL"#),

        // user_media_ratings
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".user_media_ratings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    video_item_id UUID,
    tv_show_id UUID,
    book_id UUID,
    rating DOUBLE PRECISION NOT NULL,
    review TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_ratings_user_id_video_item_id_key ON "{SCHEMA}".user_media_ratings (user_id, video_item_id) WHERE video_item_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_ratings_user_id_tv_show_id_key ON "{SCHEMA}".user_media_ratings (user_id, tv_show_id) WHERE tv_show_id IS NOT NULL"#),
        format!(r#"CREATE UNIQUE INDEX IF NOT EXISTS user_media_ratings_user_id_book_id_key ON "{SCHEMA}".user_media_ratings (user_id, book_id) WHERE book_id IS NOT NULL"#),

        // ytdlp_provider_auth
        format!(r#"CREATE TABLE IF NOT EXISTS "{SCHEMA}".ytdlp_provider_auth (
    provider TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)"#),
    ];

    for sql in &ddl {
        db.execute_raw(Statement::from_string(DatabaseBackend::Postgres, sql.clone()))
            .await?;
    }

    Ok(())
}
