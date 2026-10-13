//! HLS (`m3u8`) playlists: turn a master playlist into variants and a media
//! playlist into an ordered segment list, so the engine can assemble a stream
//! into a single file.

use url::Url;

/// One rendition in a master playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub bandwidth: u64,
    pub resolution: Option<String>,
    pub url: Url,
}

/// A media playlist: an optional init segment (fMP4) and the ordered segments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaPlaylist {
    pub init: Option<Url>,
    pub segments: Vec<Url>,
}

/// Whether a URL looks like an HLS playlist by extension.
pub fn is_hls_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.to_ascii_lowercase().ends_with(".m3u8")
}

/// Whether playlist text is a *master* playlist (lists other playlists).
pub fn is_master_playlist(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim_start().starts_with("#EXT-X-STREAM-INF"))
}

/// Parse a master playlist into its variants.
pub fn parse_master(text: &str, base: &Url) -> Vec<Variant> {
    let mut variants = Vec::new();
    let mut pending: Option<(u64, Option<String>)> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let mut bandwidth = 0u64;
            let mut resolution = None;
            for attr in rest.split(',') {
                let attr = attr.trim();
                if let Some(value) = attr.strip_prefix("BANDWIDTH=") {
                    bandwidth = value.trim().parse().unwrap_or(0);
                } else if let Some(value) = attr.strip_prefix("RESOLUTION=") {
                    resolution = Some(value.trim().to_string());
                }
            }
            pending = Some((bandwidth, resolution));
        } else if !line.starts_with('#') {
            if let Some((bandwidth, resolution)) = pending.take() {
                if let Ok(url) = base.join(line) {
                    variants.push(Variant {
                        bandwidth,
                        resolution,
                        url,
                    });
                }
            }
        }
    }
    variants
}

/// Whether a URL names a DASH manifest.
pub fn is_dash_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .map(|parsed| parsed.path().to_ascii_lowercase().ends_with(".mpd"))
        .unwrap_or(false)
}

/// The media type of a `Content-Type` header, lowercased and without parameters.
fn mime(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Whether a response is an HLS playlist served under a name that says so.
pub fn is_hls_content_type(content_type: &str) -> bool {
    matches!(
        mime(content_type).as_str(),
        "application/x-mpegurl"
            | "application/vnd.apple.mpegurl"
            | "audio/x-mpegurl"
            | "audio/mpegurl"
    )
}

/// Whether a response is a DASH manifest, which `yt-dlp` assembles for us.
pub fn is_dash_content_type(content_type: &str) -> bool {
    mime(content_type) == "application/dash+xml"
}

/// Whether a response is a media stream rather than a document.
pub fn is_media_content_type(content_type: &str) -> bool {
    let mime = mime(content_type);
    mime.starts_with("video/") || mime.starts_with("audio/")
}

/// How a response should be downloaded, when its type says so.
///
/// This is the crux of IDM-style detection: the browser reports what the player
/// actually fetched (`application/vnd.apple.mpegurl`, `application/dash+xml`,
/// `video/*`), and the engine routes on that instead of guessing from the URL.
pub fn stream_kind(content_type: &str) -> Option<crate::types::DownloadKind> {
    use crate::types::DownloadKind;
    if is_hls_content_type(content_type) {
        Some(DownloadKind::Hls)
    } else if is_dash_content_type(content_type) {
        // yt-dlp parses the manifest and assembles the representations.
        Some(DownloadKind::Ytdlp)
    } else {
        None
    }
}

/// Parse a media playlist into the init segment (if any) plus ordered segments.
pub fn parse_media(text: &str, base: &Url) -> MediaPlaylist {
    let mut playlist = MediaPlaylist::default();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let uri = rest
                .split(',')
                .find_map(|attr| attr.trim().strip_prefix("URI="))
                .map(|value| value.trim().trim_matches('"'));
            if let Some(uri) = uri {
                if let Ok(url) = base.join(uri) {
                    playlist.init = Some(url);
                }
            }
        } else if !line.starts_with('#') {
            if let Ok(url) = base.join(line) {
                playlist.segments.push(url);
            }
        }
    }

    playlist
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://cdn.example.com/hls/index.m3u8").unwrap()
    }

    #[test]
    fn detects_hls_urls() {
        assert!(is_hls_url("https://x/y/master.m3u8"));
        assert!(is_hls_url("https://x/y/master.m3u8?token=1"));
        assert!(!is_hls_url("https://x/y/movie.mp4"));
    }

    #[test]
    fn detects_dash_urls() {
        assert!(is_dash_url("https://x/y/manifest.mpd"));
        assert!(is_dash_url("https://x/y/manifest.mpd?token=1"));
        assert!(!is_dash_url("https://x/y/movie.mp4"));
    }

    #[test]
    fn classifies_streaming_content_types() {
        assert!(is_dash_content_type("application/dash+xml"));
        assert!(is_dash_content_type("application/dash+xml; charset=utf-8"));
        assert!(!is_dash_content_type("video/mp4"));
        assert!(is_hls_content_type("application/vnd.apple.mpegurl"));
        assert!(is_hls_content_type("application/x-mpegURL; charset=utf-8"));
        assert!(!is_hls_content_type("application/dash+xml"));
        // Neither is a plain file, so the engine must not save it as one.
        assert!(!is_dash_content_type("text/html"));
    }

    #[test]
    fn recognises_media_and_routes_manifests() {
        use crate::types::DownloadKind;

        assert!(is_media_content_type("video/mp4"));
        assert!(is_media_content_type("audio/webm; codecs=opus"));
        assert!(!is_media_content_type("text/html"));
        assert!(!is_media_content_type("application/zip"));

        assert_eq!(
            stream_kind("application/vnd.apple.mpegurl"),
            Some(DownloadKind::Hls)
        );
        assert_eq!(
            stream_kind("application/dash+xml; charset=utf-8"),
            Some(DownloadKind::Ytdlp)
        );
        // A plain video file is not a stream: the normal path handles it.
        assert_eq!(stream_kind("video/mp4"), None);
        assert_eq!(stream_kind("text/html"), None);
    }

    #[test]
    fn parses_a_master_playlist() {
        let text = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360
low/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080
high/index.m3u8
";
        assert!(is_master_playlist(text));
        let variants = parse_master(text, &base());
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[1].bandwidth, 5_000_000);
        assert_eq!(variants[1].resolution.as_deref(), Some("1920x1080"));
        assert_eq!(
            variants[1].url.as_str(),
            "https://cdn.example.com/hls/high/index.m3u8"
        );
    }

    #[test]
    fn parses_a_media_playlist_with_init_and_relative_segments() {
        let text = "\
#EXTM3U
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:4.0,
seg0.m4s
#EXTINF:4.0,
seg1.m4s
#EXT-X-ENDLIST
";
        assert!(!is_master_playlist(text));
        let media = parse_media(text, &base());
        assert_eq!(media.init.as_ref().unwrap().as_str(), "https://cdn.example.com/hls/init.mp4");
        assert_eq!(media.segments.len(), 2);
        assert_eq!(media.segments[1].as_str(), "https://cdn.example.com/hls/seg1.m4s");
    }

    #[test]
    fn resolves_absolute_segment_urls() {
        let text = "#EXTM3U\nhttps://other.test/a.ts\n";
        let media = parse_media(text, &base());
        assert_eq!(media.segments[0].as_str(), "https://other.test/a.ts");
    }
}
