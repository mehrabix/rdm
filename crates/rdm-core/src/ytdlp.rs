//! `yt-dlp` bridge.
//!
//! A manifest parser cannot handle the long tail of video sites (the video is
//! fetched by the player over `blob:` URLs, or extracted from a page). `yt-dlp`
//! — a mature open-source tool — does, so RDM delegates those URLs to it.
//!
//! Sites like YouTube refuse anonymous requests and need a JS runtime plus the
//! EJS challenge solver, so the caller's browser cookies and a detected
//! `node`/`deno` are passed through, and `--remote-components ejs:github`
//! enables the solver.

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
    /// JS runtime as `<name>:<path>` for the challenge solver.
    pub js_runtime: Option<&'a str>,
}

/// The `-f` selector for a maximum video height (e.g. `720`); best when `None`.
pub fn format_selector(max_height: Option<u32>) -> String {
    match max_height {
        Some(height) if height > 0 => {
            format!("bv*[height<={height}]+ba/b[height<={height}]")
        }
        _ => "bv*+ba/b".to_string(),
    }
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

/// A JS runtime yt-dlp can use for the challenges (`<name>:<path>`).
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

/// The authentication / JS-runtime flags shared by every invocation.
fn push_common(args: &mut Vec<String>, opts: &Options<'_>) {
    if let Some(js) = opts.js_runtime.filter(|v| !v.is_empty()) {
        args.push("--js-runtimes".to_string());
        args.push(js.to_string());
        // YouTube's signature/`n` challenge solver; without it yt-dlp reports
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
}

/// Arguments that download `url` into `template` (a `-o` output template).
pub fn download_args(url: &str, template: &Path, opts: &Options<'_>) -> Vec<String> {
    let mut args = vec![
        "--no-playlist".to_string(),
        "--newline".to_string(),
        "-f".to_string(),
        opts.format.unwrap_or("bv*+ba/b").to_string(),
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        "-o".to_string(),
        template.to_string_lossy().to_string(),
    ];
    push_common(&mut args, opts);
    args.push("--".to_string());
    args.push(url.to_string());
    args
}

/// Arguments that dump the media's JSON (`-J`), for listing qualities.
pub fn list_args(url: &str, opts: &Options<'_>) -> Vec<String> {
    let mut args = vec![
        "-J".to_string(),
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
    ];
    push_common(&mut args, opts);
    args.push("--".to_string());
    args.push(url.to_string());
    args
}

/// Distinct video heights advertised in a `yt-dlp -J` dump, largest first.
pub fn parse_heights(json: &str) -> Vec<u32> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut heights: Vec<u32> = value
        .get("formats")
        .and_then(|formats| formats.as_array())
        .map(|formats| {
            formats
                .iter()
                .filter_map(|format| format.get("height").and_then(|h| h.as_u64()))
                .filter(|height| *height > 0)
                .map(|height| height as u32)
                .collect()
        })
        .unwrap_or_default();
    heights.sort_unstable();
    heights.dedup();
    heights.reverse();
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_selector_uses_a_height_filter() {
        assert_eq!(format_selector(None), "bv*+ba/b");
        assert_eq!(format_selector(Some(0)), "bv*+ba/b");
        assert_eq!(
            format_selector(Some(720)),
            "bv*[height<=720]+ba/b[height<=720]"
        );
    }

    #[test]
    fn parse_heights_dedupes_and_sorts_descending() {
        let json = r#"{"formats":[
            {"format_id":"137","height":1080},
            {"format_id":"136","height":720},
            {"format_id":"18","height":360},
            {"format_id":"140","height":null,"acodec":"mp4a"},
            {"format_id":"135","height":720}
        ]}"#;
        assert_eq!(parse_heights(json), vec![1080, 720, 360]);
        assert!(parse_heights("not json").is_empty());
    }

    #[test]
    fn download_args_carry_url_template_and_format() {
        let args = download_args("https://x/w?v=1", Path::new("/tmp/o.%(ext)s"), &Options {
            format: Some("bv*[height<=1080]+ba"),
            ..Options::default()
        });
        assert!(args.iter().any(|a| a == "https://x/w?v=1"));
        assert!(args.iter().any(|a| a == "/tmp/o.%(ext)s"));
        assert!(args.windows(2).any(|w| w == ["-f", "bv*[height<=1080]+ba"]));
    }

    #[test]
    fn list_args_dump_json_and_share_the_auth_flags() {
        let opts = Options {
            cookies_from_browser: Some("chrome"),
            js_runtime: Some("node:/usr/bin/node"),
            ..Options::default()
        };
        let args = list_args("https://x/w", &opts);
        assert!(args.iter().any(|a| a == "-J"));
        assert!(args.windows(2).any(|w| w == ["--cookies-from-browser", "chrome"]));
        assert!(args.windows(2).any(|w| w == ["--remote-components", "ejs:github"]));
    }
}
