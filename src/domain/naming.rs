//! Output-filename logic: how a download is named on disk and in the
//! `Content-Disposition` header. All functions here are pure.
//!
//! Downloads are written as `<uuid>-<title>.<ext>` so that the file can still be
//! located by its unique id prefix (the extension isn't known until yt-dlp has
//! run) while carrying a human-readable title. The uuid is stripped again before
//! the name reaches a user.

/// How many characters of the video title to keep in the filename.
///
/// Plus the 36-character uuid, a separator and an extension, this stays well
/// inside the 255-byte limit a single path component gets on macOS and Linux.
const TITLE_LIMIT: usize = 100;

/// The `-o` template for a download, given the unique id for this request.
///
/// The `%(title)s` / `%(ext)s` placeholders are expanded by yt-dlp, and the
/// `.100s` precision truncates the title field.
///
/// Note: truncation belongs *here*, in the template, and not in yt-dlp's
/// `--trim-filenames`. That flag caps the length of the whole rendered output
/// path, directories included — with an absolute `-o` it silently writes the
/// file outside the requested directory, which is a bug this project has
/// already shipped once.
pub fn output_template(file_id: &str) -> String {
    format!("{file_id}-%(title).{TITLE_LIMIT}s.%(ext)s")
}

/// Turn the name yt-dlp actually produced into the name a user should see, by
/// removing the `<file_id>-` prefix.
///
/// Falls back to the stored name when stripping would leave nothing useful —
/// e.g. a video with an empty title, which yields `<uuid>-.mp3`.
pub fn display_filename(stored_name: &str, file_id: &str) -> String {
    let stripped = stored_name
        .strip_prefix(file_id)
        .map(|rest| rest.trim_start_matches('-'))
        .unwrap_or(stored_name);

    let candidate = if stripped.is_empty() || stripped.starts_with('.') {
        stored_name
    } else {
        stripped
    };

    sanitize(candidate)
}

/// Choose which of several files yt-dlp left behind is the finished download.
///
/// A download is located by its unique id prefix, because the extension isn't
/// known until yt-dlp has picked a format. Usually exactly one file matches —
/// but yt-dlp can leave an intermediate beside the result (the source file it
/// re-encoded from, kept with `-k`), and then two do. When the requested
/// quality guarantees an extension, that is the tie-break; otherwise the
/// lowest name wins, so the choice is at least deterministic rather than
/// whatever the directory happened to yield first.
///
/// `candidates` need not be sorted.
pub fn pick_output<'a>(candidates: &[&'a str], expected_ext: Option<&str>) -> Option<&'a str> {
    if let Some(ext) = expected_ext {
        let suffix = format!(".{ext}");
        if let Some(matched) = candidates
            .iter()
            .filter(|name| name.ends_with(&suffix))
            .min()
        {
            return Some(matched);
        }
    }

    candidates.iter().min().copied()
}

/// Insert ` (n)` before the extension, to avoid overwriting an existing file:
/// `Song.mp3` + 2 → `Song (2).mp3`.
pub fn numbered_name(filename: &str, n: u32) -> String {
    match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{filename} ({n})"),
    }
}

/// Strip anything that would be unsafe in a header or a path.
///
/// `--restrict-filenames` already keeps yt-dlp's output to ASCII, but a raw
/// yt-dlp format string passed through as a custom quality could in principle
/// change that, and an unquoted `"` would break the header outright.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '\\' | '/'))
        .collect();

    if cleaned.trim().is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0c4f1b8e-1111-2222-3333-444455556666";

    #[test]
    fn template_keeps_the_id_as_a_locatable_prefix() {
        let template = output_template(ID);
        assert!(template.starts_with(ID));
        assert!(template.ends_with(".%(ext)s"));
    }

    #[test]
    fn template_bounds_the_title_field_itself() {
        // A yt-dlp format precision, so only the title is truncated — never the
        // directory part of an absolute output path.
        assert!(
            output_template(ID).contains("%(title).100s"),
            "the title field must carry its own length limit"
        );
    }

    #[test]
    fn display_name_drops_the_id_prefix() {
        assert_eq!(
            display_filename(&format!("{ID}-Never_Gonna_Give_You_Up.mp3"), ID),
            "Never_Gonna_Give_You_Up.mp3"
        );
    }

    #[test]
    fn display_name_falls_back_when_the_title_was_empty() {
        // yt-dlp produced "<uuid>-.mp3" — stripping leaves only an extension.
        let stored = format!("{ID}-.mp3");
        assert_eq!(display_filename(&stored, ID), stored);
    }

    #[test]
    fn display_name_passes_through_a_name_without_the_prefix() {
        assert_eq!(display_filename("song.mp3", ID), "song.mp3");
    }

    #[test]
    fn display_name_strips_header_breaking_characters() {
        let stored = format!("{ID}-a\"b/c.mp3");
        assert_eq!(display_filename(&stored, ID), "abc.mp3");
    }

    #[test]
    fn display_name_never_returns_empty() {
        assert_eq!(display_filename("///", ID), "download");
    }

    #[test]
    fn pick_output_returns_the_only_candidate() {
        assert_eq!(
            pick_output(&["a-Song.mp3"], Some("mp3")),
            Some("a-Song.mp3")
        );
        assert_eq!(pick_output(&["a-Clip.mp4"], None), Some("a-Clip.mp4"));
    }

    #[test]
    fn pick_output_prefers_the_expected_extension() {
        // The exact case that broke: yt-dlp kept the .webm it re-encoded from,
        // and directory order could hand back the intermediate instead.
        let candidates = ["a-Song.webm", "a-Song.mp3"];
        assert_eq!(pick_output(&candidates, Some("mp3")), Some("a-Song.mp3"));

        let reversed = ["a-Song.mp3", "a-Song.webm"];
        assert_eq!(pick_output(&reversed, Some("mp3")), Some("a-Song.mp3"));
    }

    #[test]
    fn pick_output_is_deterministic_without_an_expected_extension() {
        let candidates = ["a-Song.webm", "a-Song.mkv"];
        assert_eq!(pick_output(&candidates, None), Some("a-Song.mkv"));
        assert_eq!(
            pick_output(&["a-Song.mkv", "a-Song.webm"], None),
            Some("a-Song.mkv")
        );
    }

    #[test]
    fn pick_output_falls_back_when_the_expected_extension_is_absent() {
        // Better to hand back the file that exists than to report nothing found.
        assert_eq!(
            pick_output(&["a-Song.webm"], Some("mp3")),
            Some("a-Song.webm")
        );
    }

    #[test]
    fn pick_output_of_nothing_is_none() {
        assert_eq!(pick_output(&[], Some("mp3")), None);
        assert_eq!(pick_output(&[], None), None);
    }

    #[test]
    fn pick_output_does_not_match_an_extension_mid_name() {
        // ".mp3" must be a suffix, not a substring.
        assert_eq!(
            pick_output(&["a-Song.mp3.part"], Some("mp3")),
            Some("a-Song.mp3.part")
        );
    }

    #[test]
    fn numbered_name_inserts_before_the_extension() {
        assert_eq!(numbered_name("Song.mp3", 2), "Song (2).mp3");
        assert_eq!(numbered_name("archive.tar.gz", 3), "archive.tar (3).gz");
    }

    #[test]
    fn numbered_name_handles_names_without_an_extension() {
        assert_eq!(numbered_name("Song", 2), "Song (2)");
        assert_eq!(numbered_name(".hidden", 2), ".hidden (2)");
    }
}
