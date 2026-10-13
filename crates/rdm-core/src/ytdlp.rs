//! `yt-dlp` bridge.
//!
//! A manifest parser cannot handle the long tail of video sites (the video is
//! fetched by the player over `blob:` URLs, or extracted from a page). `yt-dlp`
//! — a mature open-source tool — does, so RDM delegates those URLs to it.
//!
//! Sites like YouTube refuse anonymous requests ("Sign in to confirm you're not
//! a bot"), so the caller's cookies (forwarded from the browser extension) are
//! passed through as a `Cookie` header.

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
pub fn download_args(
    url: &str,
    template: &Path,
    format: Option<&str>,
    cookies: Option<&str>,
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "--no-playlist".to_string(),
        "--newline".to_string(),
        // Best video + best audio in one file, merging to mp4 when needed.
        "-f".to_string(),
        format.unwrap_or("bv*+ba/b").to_string(),
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        "-o".to_string(),
        template.to_string_lossy().to_string(),
    ];

    if let Some(cookies) = cookies.filter(|c| !c.trim().is_empty()) {
        args.push("--add-header".to_string());
        args.push(format!("Cookie: {cookies}"));
    }
    if let Some(user_agent) = user_agent.filter(|u| !u.trim().is_empty()) {
        args.push("--user-agent".to_string());
        args.push(user_agent.to_string());
    }
    if let Some(referer) = referer.filter(|r| !r.trim().is_empty()) {
        args.push("--add-header".to_string());
        args.push(format!("Referer: {referer}"));
    }

    args.push("--".to_string());
    args.push(url.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_carry_the_url_and_template() {
        let args = download_args(
            "https://x/watch?v=1",
            Path::new("/tmp/out.%(ext)s"),
            None,
            None,
            None,
            None,
        );
        assert!(args.iter().any(|a| a == "https://x/watch?v=1"));
        assert!(args.iter().any(|a| a == "/tmp/out.%(ext)s"));
        assert!(args.windows(2).any(|w| w == ["-f", "bv*+ba/b"]));
    }

    #[test]
    fn format_override_is_used() {
        let args = download_args("u", Path::new("t"), Some("137+140"), None, None, None);
        assert!(args.windows(2).any(|w| w == ["-f", "137+140"]));
    }

    #[test]
    fn cookies_and_headers_are_forwarded() {
        let args = download_args(
            "u",
            Path::new("t"),
            None,
            Some("SID=abc; HSID=def"),
            Some("Mozilla/5.0"),
            Some("https://x/"),
        );
        assert!(args
            .windows(2)
            .any(|w| w == ["--add-header", "Cookie: SID=abc; HSID=def"]));
        assert!(args.windows(2).any(|w| w == ["--user-agent", "Mozilla/5.0"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--add-header", "Referer: https://x/"]));
    }

    #[test]
    fn empty_cookies_are_omitted() {
        let args = download_args("u", Path::new("t"), None, Some("  "), None, None);
        assert!(!args.iter().any(|a| a.starts_with("Cookie:")));
    }
}
