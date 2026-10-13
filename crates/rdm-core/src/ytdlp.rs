//! `yt-dlp` bridge.
//!
//! A manifest parser cannot handle the long tail of video sites (the video is
//! fetched by the player over `blob:` URLs, or extracted from a page). `yt-dlp`
//! — a mature open-source tool — does, so RDM delegates those URLs to it.
//!
//! Sites like YouTube refuse anonymous requests ("Sign in to confirm you're not
//! a bot") and need a JS runtime for their challenges, so the caller's browser
//! cookies and a detected `node`/`deno` are passed through.

use std::path::{Path, PathBuf};

/// Everything RDM forwards to yt-dlp for one download.
#[derive(Default)]
pub struct Options<'a> {
    pub format: Option<&'a str>,
    pub cookies: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub referer: Option<&'a str>,
    /// Read cookies from this browser (e.g. `chrome`).
    pub cookies_from_browser: Option<&'a str>,
    /// JS runtime as `<name>:<path>` for YouTube's challenges.
    pub js_runtime: Option<&'a str>,
}

/// Directories to search, most specific first: a `pipx`/`pip --user` install in
/// `~/.local/bin` beats a distro copy, and a GUI app's `PATH` may omit it.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/bin"));
    }
    dirs.push(PathBuf::from("/usr/local/bin"));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

/// Locate `yt-dlp` (or the older `youtube-dl`).
pub fn find_binary() -> Option<PathBuf> {
    for dir in search_dirs() {
        for name in ["yt-dlp", "youtube-dl"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// A JS runtime yt-dlp can use for YouTube's challenges (`<name>:<path>`).
pub fn find_js_runtime() -> Option<String> {
    for name in ["deno", "node", "bun"] {
        for dir in search_dirs() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(format!("{name}:{}", candidate.display()));
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
pub fn download_args(url: &str, template: &Path, opts: &Options<'_>) -> Vec<String> {
    let mut args = vec![
        "--no-playlist".to_string(),
        "--newline".to_string(),
        // Best video + best audio in one file, merging to mp4 when needed.
        "-f".to_string(),
        opts.format.unwrap_or("bv*+ba/b").to_string(),
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        "-o".to_string(),
        template.to_string_lossy().to_string(),
    ];

    if let Some(js) = opts.js_runtime.filter(|v| !v.is_empty()) {
        args.push("--js-runtimes".to_string());
        args.push(js.to_string());
        // YouTube's signature/`n` challenge solver. Without it yt-dlp reports
        // "Signature solving failed" / "The page needs to be reloaded".
        args.push("--remote-components".to_string());
        args.push("ejs:github".to_string());
    }
    if let Some(browser) = opts.cookies_from_browser.filter(|v| !v.is_empty()) {
        args.push("--cookies-from-browser".to_string());
        args.push(browser.to_string());
    }
    if let Some(cookies) = opts.cookies.filter(|c| !c.trim().is_empty()) {
        args.push("--add-header".to_string());
        args.push(format!("Cookie: {cookies}"));
    }
    if let Some(user_agent) = opts.user_agent.filter(|u| !u.trim().is_empty()) {
        args.push("--user-agent".to_string());
        args.push(user_agent.to_string());
    }
    if let Some(referer) = opts.referer.filter(|r| !r.trim().is_empty()) {
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

    fn base() -> Options<'static> {
        Options::default()
    }

    #[test]
    fn args_carry_the_url_and_template() {
        let args = download_args("https://x/watch?v=1", Path::new("/tmp/o.%(ext)s"), &base());
        assert!(args.iter().any(|a| a == "https://x/watch?v=1"));
        assert!(args.iter().any(|a| a == "/tmp/o.%(ext)s"));
        assert!(args.windows(2).any(|w| w == ["-f", "bv*+ba/b"]));
    }

    #[test]
    fn format_and_headers_are_forwarded() {
        let opts = Options {
            format: Some("137+140"),
            cookies: Some("SID=abc"),
            user_agent: Some("Mozilla/5.0"),
            referer: Some("https://x/"),
            ..Options::default()
        };
        let args = download_args("u", Path::new("t"), &opts);
        assert!(args.windows(2).any(|w| w == ["-f", "137+140"]));
        assert!(args.windows(2).any(|w| w == ["--add-header", "Cookie: SID=abc"]));
        assert!(args.windows(2).any(|w| w == ["--user-agent", "Mozilla/5.0"]));
        assert!(args.windows(2).any(|w| w == ["--add-header", "Referer: https://x/"]));
    }

    #[test]
    fn browser_cookies_and_js_runtime_are_forwarded() {
        let opts = Options {
            cookies_from_browser: Some("chrome"),
            js_runtime: Some("node:/usr/bin/node"),
            ..Options::default()
        };
        let args = download_args("u", Path::new("t"), &opts);
        assert!(args.windows(2).any(|w| w == ["--cookies-from-browser", "chrome"]));
        assert!(args.windows(2).any(|w| w == ["--js-runtimes", "node:/usr/bin/node"]));
        // The JS runtime also needs the challenge-solver component.
        assert!(args.windows(2).any(|w| w == ["--remote-components", "ejs:github"]));
    }

    #[test]
    fn no_remote_components_without_a_js_runtime() {
        let args = download_args("u", Path::new("t"), &Options::default());
        assert!(!args.iter().any(|a| a == "--remote-components"));
    }

    #[test]
    fn empty_values_are_omitted() {
        let opts = Options {
            cookies: Some("  "),
            cookies_from_browser: Some(""),
            ..Options::default()
        };
        let args = download_args("u", Path::new("t"), &opts);
        assert!(!args.iter().any(|a| a.starts_with("Cookie:")));
        assert!(!args.iter().any(|a| a == "--cookies-from-browser"));
    }
}
