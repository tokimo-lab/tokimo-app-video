//! Shared helpers for queue handlers (genre sync, people/cast sync).
//! Aligned 1:1 with TS media-sync-utils.ts + tmdb-genre-catalog.ts.

use rust_client_api::metadata_providers::tmdb::{TmdbCastInfo, TmdbGenre};
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde_json::json;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;

use crate::db::entities::{genres, jobs, tv_persons, tv_season_cast, video_cast, video_persons};
use crate::db::repos::job_repo::JobRepo;

/// Check if a `DbErr` is a unique constraint violation (race condition on concurrent inserts).
pub fn is_unique_violation(err: &DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

// ── TMDB Genre Name → ID Catalog ──
// Full multilingual catalog ported 1:1 from TS tmdb-genre-catalog.ts (~30 languages per genre).

/// Resolve a genre name to its TMDB genre ID. Case-insensitive.
#[allow(clippy::too_many_lines)]
pub fn find_tmdb_genre_id_by_name(name: &str) -> Option<i32> {
    let lower = name.trim().to_lowercase();
    static CATALOG: &[(i32, &[&str])] = &[
        (
            12,
            &[
                "مغامرة",
                "Прыгоды",
                "Приключение",
                "Adventure",
                "Dobrodružný",
                "Eventyr",
                "Abenteuer",
                "Περιπέτεια",
                "Aventura",
                "Seikkailu",
                "Aventure",
                "הרפתקאות",
                "Kaland",
                "Petualangan",
                "Avventura",
                "アドベンチャー",
                "სათავგადასავლო",
                "모험",
                "Nuotykių",
                "Avontuur",
                "Przygodowy",
                "Aventuri",
                "приключения",
                "Avantura",
                "Авантуристички",
                "Äventyr",
                "ผจญ",
                "Macera",
                "Пригоди",
                "Phim Phiêu Lưu",
                "冒险",
            ],
        ),
        (
            14,
            &[
                "فانتازيا",
                "Фэнтэзі",
                "Фентъзи",
                "Fantasy",
                "Φαντασίας",
                "Fantasía",
                "Fantasia",
                "Fantastique",
                "פנטזיה",
                "Fantasi",
                "ファンタジー",
                "ფენტეზი",
                "판타지",
                "Maginė fantastika",
                "Fantasie",
                "фэнтези",
                "Fantastika",
                "Фантастика",
                "จินตนาการ",
                "Fantastik",
                "Фентезі",
                "Phim Giả Tượng",
                "奇幻",
            ],
        ),
        (
            16,
            &[
                "رسوم متحركة",
                "Мультфільм",
                "Анимация",
                "Animation",
                "Animovaný",
                "Κινούμενα Σχέδια",
                "Animación",
                "Animaatio",
                "אנימציה",
                "Animációs",
                "Animasi",
                "Animazione",
                "アニメーション",
                "მულტფილმი",
                "애니메이션",
                "Animaciniai",
                "Animatie",
                "Animacja",
                "Animação",
                "Animaţie",
                "мультфильм",
                "Animacija",
                "Цртани",
                "Animerat",
                "แอนนิเมชั่น",
                "Animasyon",
                "Phim Hoạt Hình",
                "动画",
            ],
        ),
        (
            18,
            &[
                "دراما",
                "Драма",
                "Drama",
                "Δράμα",
                "Draama",
                "Drame",
                "דרמה",
                "Dráma",
                "Dramma",
                "ドラマ",
                "დრამა",
                "드라마",
                "Dramos",
                "Dramat",
                "Dramă",
                "Drама",
                "หนังชีวิต",
                "Dram",
                "Phim Chính Kịch",
                "剧情",
            ],
        ),
        (
            27,
            &[
                "رعب",
                "Жахі",
                "Ужас",
                "Horror",
                "Horor",
                "Gyser",
                "Τρόμου",
                "Terror",
                "Kauhu",
                "Horreur",
                "אימה",
                "Kengerian",
                "ホラー",
                "საშინელება",
                "공포",
                "Siaubo",
                "ужасы",
                "Grozljivka",
                "Хорор",
                "Skräck",
                "สยองขวัญ",
                "Korku",
                "Жахи",
                "Phim Kinh Dị",
                "恐怖",
            ],
        ),
        (
            28,
            &[
                "حركة",
                "Баявік",
                "Екшън",
                "Action",
                "Akční",
                "Δράση",
                "Acción",
                "Toiminta",
                "אקשן",
                "Akció",
                "Aksi",
                "Azione",
                "アクション",
                "მძაფრსიუჟეტიანი",
                "액션",
                "Veiksmo",
                "Actie",
                "Akcja",
                "Ação",
                "Acțiune",
                "боевик",
                "Akčný",
                "Akcija",
                "Акциони",
                "บู๊",
                "Aksiyon",
                "Бойовик",
                "Phim Hành Động",
                "动作",
            ],
        ),
        (
            35,
            &[
                "كوميديا",
                "Камедыя",
                "Комедия",
                "Comedy",
                "Komedie",
                "Komödie",
                "Κωμωδία",
                "Comedia",
                "Komedia",
                "Comédie",
                "קומדיה",
                "Vígjáték",
                "Komedi",
                "Commedia",
                "コメディ",
                "კომედია",
                "코미디",
                "Komedijos",
                "Comédia",
                "Comedie",
                "Komédia",
                "Комеdija",
                "Комедија",
                "ตลก",
                "Комедія",
                "Phim Hài",
                "喜剧",
            ],
        ),
        (
            36,
            &[
                "تاريخ",
                "Гістарычны",
                "Исторически",
                "History",
                "Historický",
                "Historie",
                "Ιστορική",
                "Historia",
                "Histoire",
                "הסטוריה",
                "Történelmi",
                "Sejarah",
                "Storia",
                "履歴",
                "ისტორია",
                "역사",
                "Istoriniai",
                "Historisch",
                "Historyczny",
                "História",
                "Istoric",
                "история",
                "Zgodovinski",
                "Историјски",
                "Historisk",
                "ประวัติศาสตร์",
                "Tarih",
                "Історичний",
                "Phim Lịch Sử",
                "历史",
            ],
        ),
        (
            37,
            &[
                "غربي",
                "Вестэрн",
                "Уестърн",
                "Western",
                "Γουέστερν",
                "Lännenelokuva",
                "מערבון",
                "Barat",
                "西洋",
                "ვესტერნი",
                "서부",
                "Vesternai",
                "Faroeste",
                "вестерн",
                "Västern",
                "หนังคาวบอยตะวันตก",
                "Vahşi Batı",
                "Phim Miền Tây",
                "西部",
            ],
        ),
        (
            53,
            &[
                "إثارة",
                "Трылер",
                "Трилър",
                "Thriller",
                "Θρίλερ",
                "Suspense",
                "Trilleri",
                "מותחן",
                "Cerita Seru",
                "スリラー",
                "ტრილერი",
                "스릴러",
                "Trileriai",
                "триллер",
                "Тriler",
                "Трилер",
                "ระทึกขวัญ",
                "Gerilim",
                "Phim Gây Cấn",
                "惊悚",
            ],
        ),
        (
            80,
            &[
                "جريمة",
                "Крымінал",
                "Криминален",
                "Crime",
                "Krimi",
                "Kriminalitet",
                "Αστυνομική",
                "Crimen",
                "Rikos",
                "פשע",
                "Bűnügyi",
                "Kejahatan",
                "犯罪",
                "კრიმინალური",
                "범죄",
                "Kriminaliniai",
                "Misdaad",
                "Kryminał",
                "Crimă",
                "криминал",
                "Kriminálny",
                "Кriminalni",
                "Крими",
                "Kriminal",
                "อาชญากรรม",
                "Suç",
                "Кримінал",
                "Phim Hình Sự",
            ],
        ),
        (
            99,
            &[
                "وثائقي",
                "Дакументальны",
                "Документален",
                "Documentary",
                "Dokumentární",
                "Dokumentarfilm",
                "Ντοκυμαντέρ",
                "Documental",
                "Dokumentti",
                "Documentaire",
                "דוקומנטרי",
                "Dokumentum",
                "Dokumenter",
                "Documentario",
                "ドキュメンタリー",
                "დოკუმენტური",
                "다큐멘터리",
                "Dokumentiniai",
                "Dokumentalny",
                "Documentário",
                "Documentar",
                "документальный",
                "Dokumentárny",
                "Dokumentarni",
                "Документарни",
                "Dokumentär",
                "สารคดี",
                "Belgesel",
                "Документальний",
                "Phim Tài Liệu",
                "纪录",
                "纪录片",
            ],
        ),
        (
            878,
            &[
                "خيال علمي",
                "Фантастыка",
                "Научна-фантастика",
                "Science Fiction",
                "Vědeckofantastický",
                "Sci-fi",
                "Επ. Φαντασίας",
                "Ciencia ficción",
                "Science-Fiction",
                "מדע בדיוני",
                "Cerita Fiksi",
                "Fantascienza",
                "サイエンスフィクション",
                "ფანტასტიკა",
                "SF",
                "Mokslinė fantastika",
                "Sciencefiction",
                "Ficção científica",
                "фантастика",
                "Znanstvena fantastika",
                "Научна фантастика",
                "นิยายวิทยาศาสตร์",
                "Bilim-Kurgu",
                "Phim Khoa Học Viễn Tưởng",
                "科幻",
            ],
        ),
        (
            9648,
            &[
                "غموض",
                "Дэтэктыў",
                "Мистерия",
                "Mystery",
                "Mysteriózní",
                "Mysterium",
                "Μυστηρίου",
                "Misterio",
                "Mysteeri",
                "Mystère",
                "מסתורין",
                "Rejtély",
                "Misteri",
                "Mistero",
                "謎",
                "დეტექტივი",
                "미스터리",
                "Mistiniai",
                "Mysterie",
                "Tajemnica",
                "Mistério",
                "Mister",
                "детектив",
                "Mysteriózny",
                "Misterija",
                "Мистерија",
                "Mystik",
                "ลึกลับ",
                "Gizem",
                "Phim Bí Ẩn",
                "悬疑",
            ],
        ),
        (
            10402,
            &[
                "موسيقى",
                "Музыка",
                "Музика",
                "Music",
                "Hudební",
                "Musik",
                "Μουσική",
                "Música",
                "Musiikki",
                "Musique",
                "מוסיקה",
                "Zenei",
                "Musica",
                "音楽",
                "მუსიკა",
                "음악",
                "Muzikiniai",
                "Muziek",
                "Muzyczny",
                "Muzică",
                "Hudobný",
                "Glazbeni",
                "Музички",
                "ดนตรี",
                "Müzik",
                "Phim Nhạc",
                "音乐",
            ],
        ),
        (
            10749,
            &[
                "رومنسية",
                "Меладрама",
                "Романс",
                "Romance",
                "Romantický",
                "Romantik",
                "Liebesfilm",
                "Ρομαντική",
                "Romanssi",
                "רומנטי",
                "Romantikus",
                "Percintaan",
                "ロマンス",
                "მელოდრამა",
                "로맨스",
                "Romantiniai",
                "Romantiek",
                "Romans",
                "Romantic",
                "мелодрама",
                "Romantika",
                "Љубавни",
                "หนังรักโรแมนติก",
                "Phim Lãng Mạn",
                "爱情",
            ],
        ),
        (
            10751,
            &[
                "عائلي",
                "Сямейны",
                "Семеен",
                "Family",
                "Rodinný",
                "Familie",
                "Οικογενειακή",
                "Familia",
                "Perhe",
                "Familial",
                "משפחה",
                "Családi",
                "Keluarga",
                "Famiglia",
                "ファミリー",
                "საოჯახო",
                "가족",
                "Visai šeimai",
                "Familijny",
                "Família",
                "семейный",
                "Družinski",
                "Породични",
                "Familj",
                "ครอบครัว",
                "Aile",
                "Сімейний",
                "Phim Gia Đình",
                "家庭",
            ],
        ),
        (
            10752,
            &[
                "حرب",
                "Ваенны",
                "Военен",
                "War",
                "Válečný",
                "Krig",
                "Kriegsfilm",
                "Πολεμική",
                "Bélica",
                "Sota",
                "Guerre",
                "מלחמה",
                "Háborús",
                "Kejahatan",
                "Guerra",
                "戦争",
                "საომარი",
                "전쟁",
                "Kariniai",
                "Oorlog",
                "Wojenny",
                "Război",
                "военный",
                "Vojnový",
                "Vojno-politični",
                "Ратни",
                "สงคราม",
                "Savaş",
                "Військовий",
                "Phim Chiến Tranh",
                "战争",
            ],
        ),
        (
            10759,
            &[
                "حركة ومغامرة",
                "Экшн і Прыгоды",
                "Екшън и приключение",
                "Action & Adventure",
                "Action og eventyr",
                "Περιπέτεια - Δράσης",
                "Toiminta & Seikkailu",
                "אקשן והרפתקאות",
                "Aksi & Petualangan",
                "Veiksmo ir Nuotykių",
                "Akcja i Przygoda",
                "Acţiune & Aventuri",
                "Боевик и Приключения",
                "Akčný a Dobrodružný",
                "Akcija & Avantura",
                "Акциони и авантуристички",
                "บู๊, ผจญภัย",
                "Aksiyon & Macera",
                "Екшн і Пригоди",
                "动作冒险",
            ],
        ),
        (
            10762,
            &[
                "أطفال",
                "Дзіцячы",
                "Детски",
                "Kids",
                "Børn",
                "Παιδική",
                "Lapset",
                "ילדים",
                "Anak-anak",
                "Filmai vaikams",
                "Copii",
                "Детский",
                "Detský",
                "Otroški",
                "Дечији",
                "สำหรับเด็ก",
                "Çocuklar",
                "Дитячий",
                "儿童",
            ],
        ),
        (
            10763,
            &[
                "أخبار",
                "Навіны",
                "Новини",
                "News",
                "Nyhed",
                "Νέα",
                "Uutiset",
                "חדשות",
                "Berita",
                "Žinios",
                "Ştiri",
                "Новости",
                "Noviny",
                "Novice",
                "Вести",
                "ข่าว",
                "Haber",
                "新闻",
            ],
        ),
        (
            10764,
            &[
                "واقع",
                "Рэаліці-шоў",
                "Риалити",
                "Reality",
                "Virkelighed",
                "Ριάλιτι",
                "Tosi-TV",
                "ריאליטי",
                "Realitas",
                "Realybės šou",
                "Реалити-шоу",
                "Reality Show",
                "Resničnostni",
                "Ријалити",
                "เรียลลิตี้",
                "Gerçeklik",
                "Реаліті-шоу",
                "真人秀",
            ],
        ),
        (
            10765,
            &[
                "خيال علمي وفانتازيا",
                "Навукова фантастычны",
                "Научна-фантастика и фентъзи",
                "Sci-Fi & Fantasy",
                "Sci-fi og Fantasy",
                "Επ. Φαντασία - Φαντασίας",
                "Sci-Fi & Fantasia",
                "Science-Fiction & Fantastique",
                "מדע בדיוני ופנטזיה",
                "Mokslinė ir maginė fantastika",
                "SF & Fantasy",
                "НФ и Фэнтези",
                "Sci-Fi & Fantazija",
                "Научна фантастика",
                "จิตนิมิตแนววิทยาศาสตร์",
                "Bilim Kurgu & Fantazi",
                "Науково фантастичний",
            ],
        ),
        (
            10766,
            &[
                "أوبرا صابونية",
                "Мыльная опера",
                "Сапун",
                "Soap",
                "Sæbe",
                "סבון",
                "Sabun",
                "Melodramos",
                "Telenovela",
                "Milnica (opera)",
                "Сапунска опера",
                "ละคร",
                "Pembe Dizi",
                "Мильна опера",
                "肥皂剧",
            ],
        ),
        (
            10767,
            &[
                "حوار",
                "Ток-шоў",
                "Talk",
                "Snakke",
                "דיבורים",
                "Bicara",
                "Pokalbių šou",
                "Ток-шоу",
                "Rozhovor",
                "Pogovorni",
                "Ток шоу",
                "บทสนทนา",
                "脱口秀",
            ],
        ),
        (
            10768,
            &[
                "حرب وسياسة",
                "Палітыка і вайна",
                "Военен и политически",
                "War & Politics",
                "Krig & Politik",
                "Πολεμική - Πολιτική",
                "Sota & Politiikka",
                "מלחמה ופוליטיקה",
                "Kejahatan dan Politik",
                "Kariniai ir politiniai",
                "Război & Politică",
                "Война и Политика",
                "Vojnový a Politický",
                "Vojno-politični",
                "Ратни и политички",
                "สงครามและการเมือง",
                "Savaş & Politik",
                "Політика та війна",
            ],
        ),
        (
            10770,
            &[
                "فيلم تلفازي",
                "Тэлефільм",
                "Телевизионен филм",
                "TV Movie",
                "Televizní film",
                "TV film",
                "TV-Film",
                "τηλεοπτική ταινία",
                "Película de TV",
                "Téléfilm",
                "סרט טלויזיה",
                "Film TV",
                "televisione film",
                "テレビ映画",
                "სატელევიზიო ფილმში",
                "TV 영화",
                "Televiziniai filmai",
                "Cinema TV",
                "телевизионный фильм",
                "ТВ film",
                "ТВ филм",
                "ภาพยนตร์โทรทัศน์",
                "Телефільм",
                "Chương Trình Truyền Hình",
                "电视电影",
            ],
        ),
    ];
    for &(id, aliases) in CATALOG {
        if aliases.iter().any(|a| a.to_lowercase() == lower) {
            return Some(id);
        }
    }
    None
}

/// Sync TMDB genres to the database and link them to a movie or TV show.
pub async fn sync_genres(
    db: &DatabaseConnection,
    tmdb_genres: &[TmdbGenre],
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut genre_ids: Vec<Uuid> = Vec::new();

    for g in tmdb_genres {
        let existing = genres::Entity::find()
            .filter(genres::Column::TmdbGenreId.eq(g.id as i32))
            .one(db)
            .await?;

        let genre_uuid = if let Some(row) = existing {
            row.id
        } else {
            let id = Uuid::new_v4();
            let active = genres::ActiveModel {
                id: Set(id),
                tmdb_genre_id: Set(g.id as i32),
            };
            match genres::Entity::insert(active).exec(db).await {
                Ok(_) => id,
                Err(e) if is_unique_violation(&e) => genres::Entity::find()
                    .filter(genres::Column::TmdbGenreId.eq(g.id as i32))
                    .one(db)
                    .await?
                    .map(|r| r.id)
                    .ok_or(e)?,
                Err(e) => return Err(e.into()),
            }
        };

        genre_ids.push(genre_uuid);
    }

    link_genre_ids(db, &genre_ids, movie_id, tv_show_id).await
}

/// Sync NFO genre names to the database (name → TMDB ID lookup, then link).
/// Aligned with TS `syncGenres` which accepts string names and calls `findTmdbGenreIdByName`.
pub async fn sync_genres_from_names(
    db: &DatabaseConnection,
    genre_names: &[String],
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut genre_ids: Vec<Uuid> = Vec::new();

    for name in genre_names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some(tmdb_genre_id) = find_tmdb_genre_id_by_name(trimmed) else {
            warn!("[sync_genres] Skipping unmatched genre: {trimmed}");
            continue;
        };

        let existing = genres::Entity::find()
            .filter(genres::Column::TmdbGenreId.eq(tmdb_genre_id))
            .one(db)
            .await?;

        let genre_uuid = if let Some(row) = existing {
            row.id
        } else {
            let id = Uuid::new_v4();
            let active = genres::ActiveModel {
                id: Set(id),
                tmdb_genre_id: Set(tmdb_genre_id),
            };
            match genres::Entity::insert(active).exec(db).await {
                Ok(_) => id,
                Err(e) if is_unique_violation(&e) => genres::Entity::find()
                    .filter(genres::Column::TmdbGenreId.eq(tmdb_genre_id))
                    .one(db)
                    .await?
                    .map(|r| r.id)
                    .ok_or(e)?,
                Err(e) => return Err(e.into()),
            }
        };
        genre_ids.push(genre_uuid);
    }

    link_genre_ids(db, &genre_ids, movie_id, tv_show_id).await
}

async fn link_genre_ids(
    db: &DatabaseConnection,
    genre_ids: &[Uuid],
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if genre_ids.is_empty() {
        return Ok(());
    }

    if let Some(mid) = movie_id {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"INSERT INTO video_genres (video_item_id, genre_id)
               SELECT $1, unnest($2::uuid[])
               ON CONFLICT DO NOTHING",
            [mid.into(), genre_ids.to_vec().into()],
        );
        db.execute_raw(stmt).await?;
    }

    if let Some(tid) = tv_show_id {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r"INSERT INTO tv_show_genres (tv_show_id, genre_id)
               SELECT $1, unnest($2::uuid[])
               ON CONFLICT DO NOTHING",
            [tid.into(), genre_ids.to_vec().into()],
        );
        db.execute_raw(stmt).await?;
    }

    Ok(())
}

/// Unified people sync — aggregates cast + directors, deduplicates, creates credits.
/// Aligned 1:1 with TS `syncPeopleForMedia`.
///
/// For TV shows, `season_id` MUST be provided to link cast at the season level.
/// When `season_id` is `None` for a TV show, cast sync is skipped.
pub async fn sync_people_for_media(
    db: &DatabaseConnection,
    cast: &[CastMember],
    directors: &[String],
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
    season_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if cast.is_empty() && directors.is_empty() {
        return Ok(());
    }

    // TV cast requires a season_id — without it we can't insert into tv_season_cast
    if tv_show_id.is_some() && season_id.is_none() {
        return Ok(());
    }

    // Query existing credits to get sort_order offset (aligned with TS)
    let mut existing_set: std::collections::HashSet<String>;
    let mut sort_order: i32;

    if let Some(mid) = movie_id {
        let existing = video_cast::Entity::find()
            .filter(video_cast::Column::VideoItemId.eq(mid))
            .all(db)
            .await?;
        existing_set = existing
            .iter()
            .map(|c| format!("{}:{}", c.video_person_id, c.role))
            .collect();
        sort_order = existing.len() as i32;
    } else if let Some(sid) = season_id {
        let existing = tv_season_cast::Entity::find()
            .filter(tv_season_cast::Column::SeasonId.eq(sid))
            .all(db)
            .await?;
        existing_set = existing
            .iter()
            .map(|c| format!("{}:{}", c.tv_person_id, c.role))
            .collect();
        sort_order = existing.len() as i32;
    } else {
        return Ok(());
    }

    // Aggregate by normalized name (case-insensitive), aligned with TS
    let mut people: Vec<(String, AggregatedPerson)> = Vec::new();
    let mut key_index: HashMap<String, usize> = HashMap::new();

    let mut add_or_update = |name: &str,
                             thumb: Option<String>,
                             actor_role: Option<String>,
                             is_actor: bool,
                             is_director: bool,
                             tmdb_id: Option<i64>| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        let key = trimmed.to_lowercase();
        if let Some(&idx) = key_index.get(&key) {
            let p = &mut people[idx].1;
            if p.thumb.is_none() {
                p.thumb = thumb;
            }
            if p.actor_role.is_none() {
                p.actor_role = actor_role;
            }
            if is_actor {
                p.include_actor = true;
            }
            if is_director {
                p.include_director = true;
            }
            if p.tmdb_id.is_none() {
                p.tmdb_id = tmdb_id;
            }
        } else {
            key_index.insert(key.clone(), people.len());
            people.push((
                key,
                AggregatedPerson {
                    name: trimmed.to_string(),
                    thumb,
                    actor_role,
                    include_actor: is_actor,
                    include_director: is_director,
                    tmdb_id,
                },
            ));
        }
    };

    for m in cast {
        add_or_update(&m.name, m.thumb.clone(), m.role.clone(), true, false, m.tmdb_id);
    }
    for d in directors {
        add_or_update(d, None, None, false, true, None);
    }

    for (_key, person) in &people {
        let (person_id, needs_scrape) = if movie_id.is_some() {
            find_or_create_video_person(db, &person.name, person.thumb.as_deref(), person.tmdb_id).await?
        } else {
            find_or_create_tv_person(db, &person.name, person.thumb.as_deref(), person.tmdb_id).await?
        };

        if person.include_actor {
            let actor_key = format!("{person_id}:actor");
            if !existing_set.contains(&actor_key) {
                if let Some(mid) = movie_id {
                    let credit = video_cast::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        video_item_id: Set(mid),
                        video_person_id: Set(person_id),
                        role: Set("actor".to_string()),
                        character: Set(person.actor_role.clone()),
                        sort_order: Set(sort_order),
                    };
                    match video_cast::Entity::insert(credit).exec(db).await {
                        Ok(_) => {}
                        Err(e) if is_unique_violation(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                } else if let (Some(tid), Some(sid)) = (tv_show_id, season_id) {
                    let credit = tv_season_cast::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        tv_show_id: Set(tid),
                        season_id: Set(sid),
                        tv_person_id: Set(person_id),
                        role: Set("actor".to_string()),
                        character: Set(person.actor_role.clone()),
                        sort_order: Set(sort_order),
                    };
                    match tv_season_cast::Entity::insert(credit).exec(db).await {
                        Ok(_) => {}
                        Err(e) if is_unique_violation(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                sort_order += 1;
                existing_set.insert(actor_key);
            }
        }

        if person.include_director {
            let dir_key = format!("{person_id}:director");
            if !existing_set.contains(&dir_key) {
                if let Some(mid) = movie_id {
                    let credit = video_cast::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        video_item_id: Set(mid),
                        video_person_id: Set(person_id),
                        role: Set("director".to_string()),
                        character: Set(None),
                        sort_order: Set(sort_order),
                    };
                    match video_cast::Entity::insert(credit).exec(db).await {
                        Ok(_) => {}
                        Err(e) if is_unique_violation(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                } else if let (Some(tid), Some(sid)) = (tv_show_id, season_id) {
                    let credit = tv_season_cast::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        tv_show_id: Set(tid),
                        season_id: Set(sid),
                        tv_person_id: Set(person_id),
                        role: Set("director".to_string()),
                        character: Set(None),
                        sort_order: Set(sort_order),
                    };
                    match tv_season_cast::Entity::insert(credit).exec(db).await {
                        Ok(_) => {}
                        Err(e) if is_unique_violation(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                sort_order += 1;
                existing_set.insert(dir_key);
            }
        }

        // Dispatch person scrape when person needs detailed data
        if needs_scrape {
            let person_type = if movie_id.is_some() { "movie" } else { "tv" };
            if let Err(e) = dispatch_person_tmdb_scrape(db, person_id, person_type, movie_id, tv_show_id).await {
                warn!("person tmdb scrape dispatch failed for person {person_id}: {e}");
            }
        }
    }

    Ok(())
}

/// Uniform cast member input (covers both TMDB cast and NFO actors).
pub struct CastMember {
    pub name: String,
    pub role: Option<String>,
    pub thumb: Option<String>,
    pub tmdb_id: Option<i64>,
}

impl From<&TmdbCastInfo> for CastMember {
    fn from(c: &TmdbCastInfo) -> Self {
        CastMember {
            name: c.name.clone(),
            role: c.role.clone(),
            thumb: c.thumb.clone(),
            tmdb_id: Some(c.tmdb_id),
        }
    }
}

struct AggregatedPerson {
    name: String,
    thumb: Option<String>,
    actor_role: Option<String>,
    include_actor: bool,
    include_director: bool,
    tmdb_id: Option<i64>,
}

/// Find or create a movie person. Lookup order: `tmdb_id` → name.
/// Returns (`person_id`, `needs_scrape`).
async fn find_or_create_video_person(
    db: &DatabaseConnection,
    name: &str,
    profile_path: Option<&str>,
    tmdb_id: Option<i64>,
) -> Result<(Uuid, bool), Box<dyn std::error::Error + Send + Sync>> {
    find_or_create_person_in_table(
        db,
        name,
        profile_path,
        tmdb_id,
        video_persons::Entity::find_by_id,
        |tmdb| video_persons::Entity::find().filter(video_persons::Column::TmdbId.eq(tmdb)),
        |name| video_persons::Entity::find().filter(video_persons::Column::Name.eq(name)),
        |p: &video_persons::Model| p.id,
        |p: &video_persons::Model| p.tmdb_id.clone(),
        |p: &video_persons::Model| p.known_for_dept.clone(),
        |p: &video_persons::Model| p.profile_path.clone(),
        |id, path| {
            let mut a = video_persons::ActiveModel::new();
            a.id = sea_orm::ActiveValue::Unchanged(id);
            a.profile_path = Set(Some(path.to_string()));
            a.updated_at = Set(Some(chrono::Utc::now().fixed_offset()));
            a
        },
        |id, tmdb_str| {
            let mut a = video_persons::ActiveModel::new();
            a.id = sea_orm::ActiveValue::Unchanged(id);
            a.tmdb_id = Set(Some(tmdb_str.to_string()));
            a.updated_at = Set(Some(chrono::Utc::now().fixed_offset()));
            a
        },
        |id, name, profile_path, tmdb_id_str, now| video_persons::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            original_name: Set(None),
            aliases: Set(None),
            gender: Set(None),
            birthday: Set(None),
            birthplace: Set(None),
            profile_path: Set(profile_path.map(str::to_string)),
            profile_key: Set(None),
            biography: Set(None),
            deathday: Set(None),
            known_for_dept: Set(None),
            popularity: Set(None),
            tmdb_id: Set(tmdb_id_str),
            imdb_id: Set(None),
            javbus_id: Set(None),
            javdb_id: Set(None),
            tpdb_id: Set(None),
            metadata: Set(None),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
        },
    )
    .await
}

/// Find or create a TV person. Lookup order: `tmdb_id` → name.
/// Returns (`person_id`, `needs_scrape`).
async fn find_or_create_tv_person(
    db: &DatabaseConnection,
    name: &str,
    profile_path: Option<&str>,
    tmdb_id: Option<i64>,
) -> Result<(Uuid, bool), Box<dyn std::error::Error + Send + Sync>> {
    find_or_create_person_in_table(
        db,
        name,
        profile_path,
        tmdb_id,
        tv_persons::Entity::find_by_id,
        |tmdb| tv_persons::Entity::find().filter(tv_persons::Column::TmdbId.eq(tmdb)),
        |name| tv_persons::Entity::find().filter(tv_persons::Column::Name.eq(name)),
        |p: &tv_persons::Model| p.id,
        |p: &tv_persons::Model| p.tmdb_id.clone(),
        |p: &tv_persons::Model| p.known_for_dept.clone(),
        |p: &tv_persons::Model| p.profile_path.clone(),
        |id, path| {
            let mut a = tv_persons::ActiveModel::new();
            a.id = sea_orm::ActiveValue::Unchanged(id);
            a.profile_path = Set(Some(path.to_string()));
            a.updated_at = Set(Some(chrono::Utc::now().fixed_offset()));
            a
        },
        |id, tmdb_str| {
            let mut a = tv_persons::ActiveModel::new();
            a.id = sea_orm::ActiveValue::Unchanged(id);
            a.tmdb_id = Set(Some(tmdb_str.to_string()));
            a.updated_at = Set(Some(chrono::Utc::now().fixed_offset()));
            a
        },
        |id, name, profile_path, tmdb_id_str, now| tv_persons::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            original_name: Set(None),
            aliases: Set(None),
            gender: Set(None),
            birthday: Set(None),
            birthplace: Set(None),
            profile_path: Set(profile_path.map(str::to_string)),
            profile_key: Set(None),
            biography: Set(None),
            deathday: Set(None),
            known_for_dept: Set(None),
            popularity: Set(None),
            tmdb_id: Set(tmdb_id_str),
            tvdb_id: Set(None),
            imdb_id: Set(None),
            metadata: Set(None),
            created_at: Set(Some(now)),
            updated_at: Set(Some(now)),
        },
    )
    .await
}

/// Generic find-or-create for any person table. Avoids code duplication between
/// `find_or_create_video_person` and `find_or_create_tv_person`.
#[allow(clippy::too_many_arguments)]
async fn find_or_create_person_in_table<
    M,
    AM,
    FById,
    FByTmdb,
    FByName,
    FId,
    FTmdb,
    FDept,
    FPath,
    FUpdatePath,
    FUpdateTmdb,
    FCreate,
>(
    db: &DatabaseConnection,
    name: &str,
    profile_path: Option<&str>,
    tmdb_id: Option<i64>,
    _find_by_id: FById,
    find_by_tmdb: FByTmdb,
    find_by_name: FByName,
    get_id: FId,
    get_tmdb: FTmdb,
    get_dept: FDept,
    get_profile_path: FPath,
    make_update_path: FUpdatePath,
    make_update_tmdb: FUpdateTmdb,
    make_create: FCreate,
) -> Result<(Uuid, bool), Box<dyn std::error::Error + Send + Sync>>
where
    M: sea_orm::ModelTrait + sea_orm::IntoActiveModel<AM> + Send + Sync + Clone,
    AM: sea_orm::ActiveModelTrait<Entity: sea_orm::EntityTrait<Model = M>> + sea_orm::ActiveModelBehavior + Send + Sync,
    FById: Fn(Uuid) -> sea_orm::Select<AM::Entity>,
    FByTmdb: Fn(&str) -> sea_orm::Select<AM::Entity>,
    FByName: Fn(&str) -> sea_orm::Select<AM::Entity>,
    FId: Fn(&M) -> Uuid,
    FTmdb: Fn(&M) -> Option<String>,
    FDept: Fn(&M) -> Option<String>,
    FPath: Fn(&M) -> Option<String>,
    FUpdatePath: Fn(Uuid, &str) -> AM,
    FUpdateTmdb: Fn(Uuid, &str) -> AM,
    FCreate: Fn(Uuid, &str, Option<&str>, Option<String>, chrono::DateTime<chrono::FixedOffset>) -> AM,
{
    let tmdb_id_str = tmdb_id.map(|t| t.to_string());

    // ── Step 1: tmdb_id is the authoritative identifier ──
    if let Some(ref tid) = tmdb_id_str
        && let Some(p) = find_by_tmdb(tid.as_str()).one(db).await?
    {
        let id = get_id(&p);
        let current_path = get_profile_path(&p);
        let is_stored = current_path.as_deref().is_some_and(|pp| pp.starts_with("/storage/"));
        if let Some(path) = profile_path
            && !is_stored
            && current_path.as_deref() != Some(path)
            && let Err(e) = make_update_path(id, path).update(db).await
        {
            warn!("person upsert: failed to update profile_path for id={id}: {e}");
        }
        return Ok((id, get_dept(&p).is_none()));
    }

    // ── Step 2: Fallback — look up by name ──
    if let Some(p) = find_by_name(name).one(db).await? {
        let id = get_id(&p);
        let current_path = get_profile_path(&p);
        let is_stored = current_path.as_deref().is_some_and(|pp| pp.starts_with("/storage/"));
        if let Some(path) = profile_path
            && !is_stored
            && current_path.as_deref() != Some(path)
            && let Err(e) = make_update_path(id, path).update(db).await
        {
            warn!("person upsert: failed to update profile_path for id={id}: {e}");
        }

        let has_tmdb_id = get_tmdb(&p).is_some();
        let is_scraped = get_dept(&p).is_some();

        if !has_tmdb_id && let Some(ref tid) = tmdb_id_str {
            if let Some(existing) = find_by_tmdb(tid.as_str()).one(db).await? {
                return Ok((get_id(&existing), get_dept(&existing).is_none()));
            }
            if let Err(e) = make_update_tmdb(id, tid).update(db).await {
                warn!("person upsert: failed to update tmdb_id for id={id}: {e}");
            }
            return Ok((id, true));
        }

        return Ok((id, (has_tmdb_id || tmdb_id.is_some()) && !is_scraped));
    }

    // ── Step 3: Person doesn't exist — try INSERT, catch unique violation ──
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();
    let active = make_create(id, name, profile_path, tmdb_id_str.clone(), now);
    match <AM::Entity as sea_orm::EntityTrait>::insert(active).exec(db).await {
        Ok(_) => Ok((id, true)),
        Err(e) if is_unique_violation(&e) => {
            // Another worker inserted the same person concurrently — re-query
            if let Some(ref tid) = tmdb_id_str
                && let Some(p) = find_by_tmdb(tid.as_str()).one(db).await?
            {
                return Ok((get_id(&p), get_dept(&p).is_none()));
            }
            if let Some(p) = find_by_name(name).one(db).await? {
                return Ok((
                    get_id(&p),
                    (get_tmdb(&p).is_some() || tmdb_id.is_some()) && get_dept(&p).is_none(),
                ));
            }
            Err(e.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// Check if a person scrape should be dispatched (not yet scraped + no active job).
async fn should_dispatch_person_scrape(
    db: &DatabaseConnection,
    person_id: Uuid,
    person_type: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let known_for_dept = if person_type == "movie" {
        video_persons::Entity::find_by_id(person_id)
            .one(db)
            .await?
            .and_then(|p| p.known_for_dept)
    } else {
        tv_persons::Entity::find_by_id(person_id)
            .one(db)
            .await?
            .and_then(|p| p.known_for_dept)
    };

    if known_for_dept.is_some() {
        return Ok(false);
    }

    let has_active_job = jobs::Entity::find()
        .filter(jobs::Column::Type.eq("tmdb_person_scrape"))
        .filter(
            sea_orm::sea_query::Condition::any()
                .add(jobs::Column::Status.eq("pending"))
                .add(jobs::Column::Status.eq("running")),
        )
        .filter(Expr::cust_with_values(
            r#""jobs"."payload"->>'personId' = $1"#,
            [person_id.to_string()],
        ))
        .count(db)
        .await?;

    Ok(has_active_job == 0)
}

/// Dispatch `tmdb_person_scrape` job with active-job dedup.
pub async fn dispatch_person_tmdb_scrape(
    db: &DatabaseConnection,
    person_id: Uuid,
    person_type: &str,
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !should_dispatch_person_scrape(db, person_id, person_type).await? {
        return Ok(());
    }
    create_person_scrape_job(db, person_id, person_type, movie_id, tv_show_id).await
}

async fn create_person_scrape_job(
    db: &DatabaseConnection,
    person_id: Uuid,
    person_type: &str,
    movie_id: Option<Uuid>,
    tv_show_id: Option<Uuid>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = JobRepo::create_job(
        db,
        "tmdb_person_scrape",
        json!({
            "personId": person_id.to_string(),
            "personType": person_type,
            "movieId": movie_id.map(|u| u.to_string()),
            "tvShowId": tv_show_id.map(|u| u.to_string()),
        }),
        None,
        None,
    )
    .await;
    Ok(())
}
