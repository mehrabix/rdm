//! `yt-dlp` bridge.
//!
//! A manifest parser cannot handle the long tail of video sites (the video is
//! fetched by the player over `blob:` URLs, or extracted from a page). `yt-dlp`
//! — a mature open-source tool — does, so RDM delegates those URLs to it.

use std::path::{Path, PathBuf};

/// Locate `yt-dlp` (or the older `youtube-dl`) on `PATH`.
pub fn find_binary() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in ["yt-dlp", "youtube-dl"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Whether a downloader binary is available.
pub fn is_available() -> bool {
    find_binary().is_some()
}

/// Arguments that download `url` into `template` (a `-o` output template).
pub fn download_args(url: &str, template: &Path, format: Option<&str>) -> Vec<String> {
    vec![
        "--no-playlist".to_string(),
        "--newline".to_string(),
        // Best video + best audio in one file, merging to mp4 when needed.
        "-f".to_string(),
        format.unwrap_or("bv*+ba/b").to_string(),
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        "-o".to_string(),
        template.to_string_lossy().to_string(),
        "--".to_string(),
        url.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_carry_the_url_and_template() {
        let args = download_args("https://x/watch?v=1", Path::new("/tmp/out.%(ext)s"), None);
        assert!(args.iter().any(|a| a == "https://x/watch?v=1"));
        assert!(args.iter().any(|a| a == "/tmp/out.%(ext)s"));
        assert!(args.windows(2).any(|w| w == ["-f", "bv*+ba/b"]));
    }

    #[test]
    fn format_override_is_used() {
        let args = download_args("u", Path::new("t"), Some("137+140"));
        assert!(args.windows(2).any(|w| w == ["-f", "137+140"]));
    }
}
