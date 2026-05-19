/**
 * Media organize format defaults — local copy.
 *
 * TODO: extract to a shared @tokimo/media-organize package once music /
 * photo / book apps are also extracted; until then video keeps a private
 * copy to avoid coupling to the OS SDK root export.
 */

const DEFAULT_MOVIE_FOLDER = "{{name}} ({{year}})";
const DEFAULT_MOVIE_FILE =
  "{{name}} ({{year}}){% if version %} - {{version}}{% endif %}";
const DEFAULT_TV_FOLDER = "{{name}} ({{year}})";
const DEFAULT_TV_FILE =
  "{{name}} S{{season}}E{{ep_start}}{% if ep_end %}-E{{ep_end}}{% endif %}{% if version %} - {{version}}{% endif %}";
const DEFAULT_ADULT_FOLDER = "{{series}}/{{video_id}}";
const DEFAULT_ADULT_FILE = "{{video_id}}";
const DEFAULT_MUSIC_FOLDER =
  "{{artist}}/{{album}}{% if year %} ({{year}}){% endif %}";
const DEFAULT_MUSIC_FILE = "{{track}}. {{title}}";
const DEFAULT_ONLINE_VIDEO_FOLDER = "{{source_site}}/{{source_id}}";
const DEFAULT_ONLINE_VIDEO_FILE = "{{source_id}}";

export function getDefaultFolderFormat(ct: string): string {
  switch (ct) {
    case "movie":
    case "documentary":
      return DEFAULT_MOVIE_FOLDER;
    case "tv":
    case "anime":
    case "variety":
      return DEFAULT_TV_FOLDER;
    case "adult":
      return DEFAULT_ADULT_FOLDER;
    case "music":
    case "audiobook":
    case "podcast":
      return DEFAULT_MUSIC_FOLDER;
    case "online_video":
    case "concert":
    case "online_course":
      return DEFAULT_ONLINE_VIDEO_FOLDER;
    default:
      return DEFAULT_MOVIE_FOLDER;
  }
}

export function getDefaultFileFormat(ct: string): string {
  switch (ct) {
    case "movie":
    case "documentary":
      return DEFAULT_MOVIE_FILE;
    case "tv":
    case "anime":
    case "variety":
      return DEFAULT_TV_FILE;
    case "adult":
      return DEFAULT_ADULT_FILE;
    case "music":
    case "audiobook":
    case "podcast":
      return DEFAULT_MUSIC_FILE;
    case "online_video":
    case "concert":
    case "online_course":
      return DEFAULT_ONLINE_VIDEO_FILE;
    default:
      return DEFAULT_MOVIE_FILE;
  }
}
