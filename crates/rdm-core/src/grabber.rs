//! Site grabber: crawl a page (depth-limited, optionally same-origin) and
//! collect the links worth downloading, with include/exclude filters.
//!
//! The HTML scan is deliberately dependency-free: it looks for `href`/`src`
//! attribute values, which is what IDM's site grabber keys off too. Anything
//! ambiguous (JS-built links, `<base>` handling) is skipped rather than guessed.

use serde::{Deserialize, Serialize};
use url::Url;

/// What a discovered link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoundKind {
    /// A page to crawl, not to download.
    Page,
    Video,
    Audio,
    Image,
    Archive,
    Document,
    Other,
}

impl FoundKind {
    /// Whether this is something the grabber would download.
    pub fn is_downloadable(self) -> bool {
        !matches!(self, FoundKind::Page)
    }
}

/// One discovered link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub url: String,
    pub kind: FoundKind,
}

/// How far to crawl and what to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrabOptions {
    /// How many links deep to follow (0 = the start page only).
    pub depth: u8,
    /// Stay on the start page's origin while crawling.
    pub same_origin: bool,
    /// Keep only URLs matching one of these globs (empty = keep all).
    pub include: Vec<String>,
    /// Drop URLs matching any of these globs.
    pub exclude: Vec<String>,
    /// Stop after this many downloads have been found.
    pub limit: usize,
}

impl Default for GrabOptions {
    fn default() -> Self {
        Self {
            depth: 1,
            same_origin: true,
            include: Vec::new(),
            exclude: Vec::new(),
            limit: 500,
        }
    }
}

const VIDEO: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "mov", "avi", "flv", "mpg", "mpeg", "ts", "m3u8", "mpd",
];
const AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "flac", "ogg", "oga", "opus", "wav", "wma",
];
const IMAGE: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tif", "tiff", "avif",
];
const ARCHIVE: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "iso", "dmg", "deb", "rpm", "apk",
];
const DOCUMENT: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "epub", "rtf", "odt",
];
const PAGE: &[&str] = &["html", "htm", "php", "asp", "aspx", "jsp", "cgi", "xhtml", "shtml"];

/// The URL path's extension, lowercased and without the dot.
pub fn extension(url: &Url) -> Option<String> {
    let path = url.path();
    let last = path.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => Some(ext.to_ascii_lowercase()),
        _ => None,
    }
}

/// Classify a link by its extension (no extension ⇒ a page).
pub fn classify(url: &Url) -> FoundKind {
    let Some(ext) = extension(url) else {
        return FoundKind::Page;
    };
    let has = |set: &[&str]| set.contains(&ext.as_str());
    if has(VIDEO) {
        FoundKind::Video
    } else if has(AUDIO) {
        FoundKind::Audio
    } else if has(IMAGE) {
        FoundKind::Image
    } else if has(ARCHIVE) {
        FoundKind::Archive
    } else if has(DOCUMENT) {
        FoundKind::Document
    } else if has(PAGE) {
        FoundKind::Page
    } else {
        FoundKind::Other
    }
}

/// Case-insensitive glob match where `*` matches any run of characters.
pub fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = value.as_str();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(position) = rest.find(part) else {
            return false;
        };
        // The first part must match at the start unless the pattern began `*`.
        if index == 0 && position != 0 {
            return false;
        }
        rest = &rest[position + part.len()..];
    }
    pattern.ends_with('*') || rest.is_empty()
}

/// Whether a URL survives the include/exclude filters.
pub fn wanted(url: &str, opts: &GrabOptions) -> bool {
    if opts.exclude.iter().any(|pattern| glob_matches(pattern, url)) {
        return false;
    }
    opts.include.is_empty() || opts.include.iter().any(|pattern| glob_matches(pattern, url))
}

/// Keep the downloadable links the filters allow, capped at `limit`.
pub fn select(found: &[Found], opts: &GrabOptions) -> Vec<Found> {
    found
        .iter()
        .filter(|item| item.kind.is_downloadable() && wanted(&item.url, opts))
        .take(opts.limit)
        .cloned()
        .collect()
}

/// Values of an attribute in an HTML document (`href`, `src`, …).
fn attribute_values(html: &str, attribute: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut values = Vec::new();
    let mut from = 0;

    while let Some(offset) = lower[from..].find(attribute) {
        let index = from + offset;
        from = index + attribute.len();

        // Must be a whole attribute name, e.g. not `data-href`.
        let boundary = index == 0
            || matches!(bytes[index - 1], b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'"' | b'\'');
        let mut cursor = from;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            cursor += 1;
        }
        if !boundary || bytes.get(cursor) != Some(&b'=') {
            continue;
        }
        cursor += 1;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            cursor += 1;
        }

        let (start, end) = match bytes.get(cursor) {
            Some(quote @ (b'"' | b'\'')) => {
                let start = cursor + 1;
                let end = bytes[start..]
                    .iter()
                    .position(|byte| byte == quote)
                    .map(|len| start + len)
                    .unwrap_or(bytes.len());
                (start, end)
            }
            _ => {
                let start = cursor;
                let end = bytes[start..]
                    .iter()
                    .position(|byte| matches!(byte, b' ' | b'>' | b'\n' | b'\r'))
                    .map(|len| start + len)
                    .unwrap_or(bytes.len());
                (start, end)
            }
        };
        if end > start {
            values.push(html[start..end].to_string());
        }
    }
    values
}

/// Every absolute `http(s)` link in a document, de-duplicated, in order.
pub fn extract_links(html: &str, base: &Url) -> Vec<Url> {
    let mut links: Vec<Url> = Vec::new();
    for value in attribute_values(html, "href").into_iter().chain(attribute_values(html, "src")) {
        let value = value.trim();
        if value.is_empty() || value.starts_with("data:") || value.starts_with("javascript:") {
            continue;
        }
        if let Ok(joined) = base.join(value) {
            if matches!(joined.scheme(), "http" | "https") && !links.contains(&joined) {
                links.push(joined);
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://site.test/docs/index.html").unwrap()
    }

    #[test]
    fn classifies_by_extension() {
        let kind = |u: &str| classify(&Url::parse(u).unwrap());
        assert_eq!(kind("https://x/a/b.mp4"), FoundKind::Video);
        assert_eq!(kind("https://x/a/b.m3u8"), FoundKind::Video);
        assert_eq!(kind("https://x/a/song.mp3"), FoundKind::Audio);
        assert_eq!(kind("https://x/a/pic.jpeg"), FoundKind::Image);
        assert_eq!(kind("https://x/a/pack.zip"), FoundKind::Archive);
        assert_eq!(kind("https://x/a/report.pdf"), FoundKind::Document);
        assert_eq!(kind("https://x/a/page.html"), FoundKind::Page);
        assert_eq!(kind("https://x/a/about"), FoundKind::Page);
        assert_eq!(kind("https://x/a/file.bin"), FoundKind::Other);
    }

    #[test]
    fn extracts_links_from_common_attributes() {
        let html = r#"
            <html><head><link rel="stylesheet" href="/style.css"></head>
            <body>
              <a href="sub/page.html">next</a>
              <a href='https://other.test/abs.mp4'>abs</a>
              <img src="images/pic.png">
              <video src="videos/clip.mp4"></video>
              <a href="mailto:x@y.z">mail</a>
              <a href="javascript:void(0)">js</a>
              <a data-href="/ignored.html">not an href</a>
            </body></html>"#;
        let links: Vec<String> = extract_links(html, &base())
            .iter()
            .map(Url::to_string)
            .collect();
        assert!(links.contains(&"https://site.test/style.css".to_string()));
        assert!(links.contains(&"https://site.test/docs/sub/page.html".to_string()));
        assert!(links.contains(&"https://other.test/abs.mp4".to_string()));
        assert!(links.contains(&"https://site.test/docs/images/pic.png".to_string()));
        assert!(links.contains(&"https://site.test/docs/videos/clip.mp4".to_string()));
        assert!(!links.iter().any(|link| link.contains("mailto")));
        assert!(!links.iter().any(|link| link.contains("javascript")));
        assert!(!links.iter().any(|link| link.contains("ignored")));
    }

    #[test]
    fn globs_anchor_correctly() {
        assert!(glob_matches("*.mp4", "https://x/a.mp4"));
        assert!(!glob_matches("*.mp4", "https://x/a.mp4?x=1"));
        assert!(glob_matches("*/video/*", "https://x/video/a.mp4"));
        assert!(!glob_matches("*/video/*", "https://x/audio/a.mp4"));
        assert!(glob_matches("https://x/*", "https://x/anything"));
        assert!(glob_matches("a.mp4", "A.MP4"));
        assert!(!glob_matches("", "anything"));
    }

    #[test]
    fn filters_and_limit_apply_to_downloadable_links() {
        let found = vec![
            Found { url: "https://x/index.html".into(), kind: FoundKind::Page },
            Found { url: "https://x/a.mp4".into(), kind: FoundKind::Video },
            Found { url: "https://x/ad.mp4".into(), kind: FoundKind::Video },
            Found { url: "https://x/b.pdf".into(), kind: FoundKind::Document },
        ];
        let opts = GrabOptions {
            include: vec!["*.mp4".into()],
            exclude: vec!["*/ad.*".into()],
            ..GrabOptions::default()
        };
        let selected = select(&found, &opts);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].url, "https://x/a.mp4");

        let capped = select(&found, &GrabOptions { limit: 2, ..GrabOptions::default() });
        assert_eq!(capped.len(), 2);
        assert!(capped.iter().all(|item| item.kind.is_downloadable()));
    }
}
