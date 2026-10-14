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

/// Directories a version manager installs into, which a GUI app's `PATH` — set
/// by the session, not the user's shell — usually omits. Newest nvm first.
fn version_manager_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().join("bin"))
            .collect();
        versions.sort();
        versions.reverse();
        dirs.extend(versions);
    }
    for dir in [
        ".volta/bin",
        ".deno/bin",
        ".bun/bin",
        ".local/share/pnpm",
        ".npm-global/bin",
        ".cargo/bin",
    ] {
        dirs.push(home.join(dir));
    }
    dirs
}

/// Directories to search, most specific first: a `pipx`/`pip --user` install in
/// `~/.local/bin` beats a distro copy, and a GUI app's `PATH` may omit it.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(&home);
        dirs.push(home.join(".local/bin"));
        dirs.extend(version_manager_dirs(&home));
    }
    for dir in ["/usr/local/bin", "/snap/bin", "/opt/homebrew/bin"] {
        dirs.push(PathBuf::from(dir));
    }
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

/// Browser profile directories to probe, in order, as `(yt-dlp name, path)`.
///
/// Sites like YouTube refuse anonymous extraction outright, and the user's own
/// browser already holds the signed-in session — so when nothing is configured,
/// the first browser that is actually installed is the right cookie source.
fn browser_candidates(home: &Path) -> Vec<(&'static str, PathBuf)> {
    [
        ("firefox", ".mozilla/firefox"),
        ("firefox", "snap/firefox/common/.mozilla/firefox"),
        ("chrome", ".config/google-chrome"),
        ("chromium", ".config/chromium"),
        ("edge", ".config/microsoft-edge"),
        ("brave", ".config/BraveSoftware/Brave-Browser"),
    ]
    .into_iter()
    .map(|(name, dir)| (name, home.join(dir)))
    .collect()
}

/// The browser yt-dlp should read cookies from when the user has not chosen one.
pub fn detect_browser() -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    browser_candidates(&home)
        .into_iter()
        .find(|(_, dir)| dir.is_dir())
        .map(|(name, _)| name.to_string())
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

/// Whether a URL points at YouTube. Host-based and conservative, like the site
/// detection in the good extractor front-ends: a CDN URL that merely mentions
/// "youtube" in a query string is not YouTube, and must not get its arguments.
pub fn is_youtube(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
}

/// Per-site extractor arguments. YouTube's default client answers with a single
/// 360p format (measured: 5 formats, heights `[360]`), while `web_safari`
/// returns the whole ladder — `[1080, 720, 480, 360, 240, 144]` — for the same
/// session, which is what makes a quality picker mean anything. `default` stays
/// in the list as the fallback for videos where the Safari client is refused.
pub fn extractor_args(url: &str) -> Option<&'static str> {
    is_youtube(url).then_some("youtube:player_client=web_safari,default")
}

/// The authentication / JS-runtime flags shared by every invocation.
fn push_common(args: &mut Vec<String>, url: &str, opts: &Options<'_>) {
    if let Some(extractor) = extractor_args(url) {
        args.push("--extractor-args".to_string());
        args.push(extractor.to_string());
    }
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
    push_common(&mut args, url, opts);
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
    push_common(&mut args, url, opts);
    args.push("--".to_string());
    args.push(url.to_string());
    args
}

/// One `[download]` progress line from yt-dlp, as far as it can be understood.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub percent: f64,
    pub total: Option<u64>,
    pub downloaded: Option<u64>,
    /// Bytes per second.
    pub speed: Option<f64>,
    pub eta_secs: Option<u64>,
}

/// Parse a yt-dlp progress line: `--newline` gives one per update, like
/// `[download]  12.3% of  123.45MiB at  1.23MiB/s ETA 00:12`.
///
/// yt-dlp is not our own transfer loop, so this is the only way its progress
/// reaches the table — without it a download in flight looks dead.
pub fn parse_progress(line: &str) -> Option<Progress> {
    let rest = line.trim().strip_prefix("[download]")?.trim();
    let percent: f64 = rest.split('%').next()?.trim().parse().ok()?;

    let mut total = None;
    if let Some(index) = rest.find(" of ") {
        let tail = &rest[index + 4..];
        let size = tail.split(" at ").next()?.split(" in ").next()?.trim();
        total = parse_size(size.trim_start_matches(['~', '\u{2248}']).trim());
    }

    let mut speed = None;
    if let Some(index) = rest.find(" at ") {
        let tail = &rest[index + 4..];
        if let Some(value) = tail.split_whitespace().next() {
            speed = parse_size(value.strip_suffix("/s").unwrap_or(value))
                .map(|bytes| bytes as f64);
        }
    }

    let eta_secs = rest
        .find("ETA ")
        .and_then(|index| parse_clock(rest[index + 4..].trim()));

    Some(Progress {
        percent,
        total,
        downloaded: total.map(|total| ((percent / 100.0) * total as f64) as u64),
        speed,
        eta_secs,
    })
}

/// `"123.45MiB"` / `"1.5GiB"` / `"900B"` as bytes.
fn parse_size(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let number: f64 = number.trim().parse().ok()?;
    let factor = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0 * 1024.0,
        "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * factor) as u64)
}

/// `"00:12"` or `"01:02:03"` as seconds.
fn parse_clock(value: &str) -> Option<u64> {
    let mut seconds = 0u64;
    let mut parts = 0;
    for part in value.split(':') {
        let part: u64 = part.trim().parse().ok()?;
        seconds = seconds * 60 + part;
        parts += 1;
    }
    (parts > 0).then_some(seconds)
}

/// A short, actionable hint for the errors that need one, else `None`.
///
/// yt-dlp's own messages say *what* failed but not what RDM needs from you.
pub fn hint(stderr: &str) -> Option<&'static str> {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("innertube_context") || lower.contains("player response") {
        // YouTube refused the client: an extractor crash here means it never got
        // a usable page, which is what a missing session looks like.
        Some("YouTube refused that request. Sign in to YouTube in your browser, set it under Options → Browser, and install node or deno so yt-dlp can solve the player challenge.")
    } else if lower.contains("needs to be reloaded") || lower.contains("signature solving") {
        Some("Install node or deno: yt-dlp needs a JavaScript runtime to solve this site's player challenge.")
    } else if lower.contains("confirm you") && lower.contains("not a bot") {
        Some("Sign in to the site in your browser, and install node or deno so yt-dlp can use cookies and solve challenges.")
    } else if lower.contains("impersonate") {
        Some("Install yt-dlp with impersonation support: `pipx install 'yt-dlp[default,curl-cffi]'`.")
    } else if lower.contains("ffmpeg") && lower.contains("not found") {
        Some("Install ffmpeg: it is required to merge separate video and audio tracks.")
    } else {
        None
    }
}

/// Distinct video heights advertised in a `yt-dlp -J` dump, largest first.
///
/// Only *video* formats count. yt-dlp also lists storyboards — sprite sheets
/// with a `height` and no codec (`sb0`, `ext: mhtml`, `format_note: storyboard`)
/// — and treating those as resolutions is how a 360p video ended up offering
/// "27p, 45p, 90p, 180p".
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
                .filter(|format| is_video_format(format))
                .filter_map(|format| {
                    format
                        .get("height")
                        .and_then(|h| h.as_u64())
                        .or_else(|| {
                            format
                                .get("resolution")
                                .and_then(|r| r.as_str())
                                .and_then(resolution_height)
                        })
                })
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

/// The height in a `"640x360"` resolution string.
fn resolution_height(resolution: &str) -> Option<u64> {
    let (_, height) = resolution.split_once('x')?;
    height.trim().parse().ok()
}

/// Whether a format entry is a real video stream: it must carry a video codec,
/// and must not be a storyboard sprite sheet.
fn is_video_format(format: &serde_json::Value) -> bool {
    let text = |key: &str| format.get(key).and_then(|v| v.as_str()).unwrap_or("");
    let codec = text("vcodec");
    if codec.is_empty() || codec == "none" {
        return false;
    }
    if text("format_note").eq_ignore_ascii_case("storyboard") {
        return false;
    }
    text("ext") != "mhtml"
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
    fn version_manager_dirs_prefer_the_newest_nvm() {
        let home = std::env::temp_dir().join(format!("rdm-nvm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for version in ["v18.0.0", "v20.1.0"] {
            std::fs::create_dir_all(
                home.join(".nvm/versions/node").join(version).join("bin"),
            )
            .unwrap();
        }
        let dirs = version_manager_dirs(&home);
        assert!(dirs[0].ends_with("v20.1.0/bin"), "got {:?}", dirs.first());
        assert!(dirs.iter().any(|d| d.ends_with(".volta/bin")));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn hint_names_the_tool_to_install() {
        assert!(hint("ERROR: [youtube] x: The page needs to be reloaded.")
            .unwrap()
            .contains("node"));
        assert!(hint("Sign in to confirm you're not a bot")
            .unwrap()
            .contains("Sign in"));
        assert!(hint("ffmpeg not found").unwrap().contains("ffmpeg"));
        assert!(hint("some unrelated failure").is_none());
    }

    /// The confusing YouTube failure — an `INNERTUBE_CONTEXT` crash instead of a
    /// clean "sign in" — must still tell the user what to do.
    #[test]
    fn hint_explains_a_refused_youtube_client() {
        let hint = hint("ERROR: [youtube] x: An extractor error has occurred. (caused by KeyError('INNERTUBE_CONTEXT'))")
            .expect("the internal-context crash needs a hint");
        assert!(hint.contains("Sign in"), "{hint}");
        assert!(hint.contains("Options → Browser"), "{hint}");
    }

    #[test]
    fn browser_candidates_cover_the_common_installs() {
        let home = std::env::temp_dir().join(format!("rdm-browsers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let names: Vec<&str> = browser_candidates(&home).iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"firefox"));
        assert!(names.contains(&"chrome"));
        assert!(names.contains(&"chromium"));
        // Nothing installed under an empty home: no candidate exists.
        assert!(browser_candidates(&home).iter().all(|(_, dir)| !dir.is_dir()));

        std::fs::create_dir_all(home.join(".config/google-chrome")).unwrap();
        let found = browser_candidates(&home)
            .into_iter()
            .find(|(_, dir)| dir.is_dir())
            .map(|(name, _)| name.to_string());
        assert_eq!(found.as_deref(), Some("chrome"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn parse_heights_dedupes_and_sorts_descending() {
        let json = r#"{"formats":[
            {"format_id":"137","height":1080,"vcodec":"avc1","ext":"mp4"},
            {"format_id":"136","height":720,"vcodec":"avc1","ext":"mp4"},
            {"format_id":"18","height":360,"vcodec":"avc1","ext":"mp4"},
            {"format_id":"140","height":null,"acodec":"mp4a","vcodec":"none","ext":"m4a"},
            {"format_id":"135","height":720,"vcodec":"avc1","ext":"mp4"}
        ]}"#;
        assert_eq!(parse_heights(json), vec![1080, 720, 360]);
        assert!(parse_heights("not json").is_empty());
    }

    /// The bug behind an unreadable quality list: storyboards are not video, and
    /// a format may carry its size only as a `"640x360"` string.
    #[test]
    fn qualities_ignore_storyboards_and_read_the_resolution() {
        let json = r#"{"formats":[
            {"format_id":"sb0","ext":"mhtml","height":180,"resolution":"270x180","format_note":"storyboard","vcodec":"none","acodec":"none"},
            {"format_id":"sb3","ext":"mhtml","height":27,"resolution":"48x27","format_note":"storyboard","vcodec":"none","acodec":"none"},
            {"format_id":"18","ext":"mp4","height":360,"resolution":"540x360","format_note":"360p","vcodec":"avc1.42001E","acodec":"mp4a.40.2"},
            {"format_id":"22","ext":"mp4","resolution":"1280x720","vcodec":"avc1.64001F","acodec":"mp4a.40.2"}
        ]}"#;
        assert_eq!(parse_heights(json), vec![720, 360]);
    }

    /// yt-dlp's own numbers are the only progress the table gets for a download
    /// it runs, so the parser has to cope with the shapes it prints.
    #[test]
    fn progress_lines_are_understood() {
        let update = parse_progress("[download]  12.3% of  123.45MiB at  1.23MiB/s ETA 00:12")
            .expect("a mid-download line");
        assert!((update.percent - 12.3).abs() < 0.001);
        assert_eq!(update.total, Some(129_446_707));
        assert_eq!(update.downloaded, Some(15_921_944));
        assert_eq!(update.speed, Some(1_289_748.0));
        assert_eq!(update.eta_secs, Some(12));

        assert_eq!(parse_progress("[download] 100% of 900B in 00:01").unwrap().total, Some(900));
        assert_eq!(
            parse_progress("[download]  50.0% of ~1.50GiB at Unknown ETA Unknown")
                .unwrap()
                .speed,
            None
        );
        assert_eq!(
            parse_progress("[download]  50.0% of 1.50GiB at 2.00MiB/s ETA 01:02:03")
                .unwrap()
                .eta_secs,
            Some(3723)
        );

        assert!(parse_progress("[info] Downloading 1 format(s): 18").is_none());
        assert!(parse_progress("").is_none());
    }

    /// The measured difference: YouTube's default client exposes one 360p
    /// format, `web_safari` exposes the whole ladder. Both the download and the
    /// quality probe must ask for it, and only for YouTube.
    #[test]
    fn youtube_asks_for_the_client_that_has_every_quality() {
        assert!(is_youtube("https://www.youtube.com/watch?v=x"));
        assert!(is_youtube("https://youtu.be/x"));
        assert!(is_youtube("https://m.youtube.com/watch?v=x"));
        assert!(is_youtube("https://www.youtube-nocookie.com/embed/x"));
        // A CDN URL that merely mentions YouTube is not YouTube.
        assert!(!is_youtube("https://cdn.example.test/youtube/clip.mp4"));
        assert!(!is_youtube("https://example.test/?ref=youtube.com"));
        assert!(!is_youtube("not a url"));

        let opts = Options::default();
        let ladder = ["--extractor-args", "youtube:player_client=web_safari,default"];
        let download = download_args(
            "https://www.youtube.com/watch?v=x",
            Path::new("/tmp/o.%(ext)s"),
            &opts,
        );
        assert!(download.windows(2).any(|w| w == ladder), "{download:?}");
        let list = list_args("https://www.youtube.com/watch?v=x", &opts);
        assert!(list.windows(2).any(|w| w == ladder), "{list:?}");
        // Every other site is left to yt-dlp's own defaults.
        let other = list_args("https://vimeo.com/1", &opts);
        assert!(!other.iter().any(|a| a == "--extractor-args"), "{other:?}");
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
