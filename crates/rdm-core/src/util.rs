//! Small filesystem/URL helpers.

use std::path::{Path, PathBuf};

/// Strip characters that are illegal in file names on common platforms and
/// collapse whitespace. Never returns an empty string.
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

/// Derive a plausible file name from a URL path, falling back to `"download"`.
pub fn filename_from_url(url: &str) -> String {
    // Prefer a real URL parse so the scheme/host are stripped correctly;
    // fall back to naive splitting for relative inputs.
    let path = url::Url::parse(url)
        .map(|parsed| parsed.path().to_string())
        .unwrap_or_else(|_| {
            url.split(['?', '#'])
                .next()
                .unwrap_or(url)
                .to_string()
        });

    let last = path
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");

    let decoded = percent_decode(last);
    sanitize_filename(if decoded.is_empty() { "download" } else { &decoded })
}

/// Minimal percent-decoding for URL path segments (`%20` -> space).
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Avoid clobbering an existing file by appending ` (n)` before the extension.
/// The returned path is guaranteed not to exist *at call time*.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = path.extension().and_then(|s| s.to_str());
    for n in 1..u32::MAX {
        let file = match ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(file);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(name)
}

/// Stems that name a *route*, not a file: `youtube.com/watch?v=…` would
/// otherwise be saved as `watch.mp4`. When the URL stem is one of these, the
/// page's own title is a far better name.
const GENERIC_STEMS: &[&str] = &[
    "watch", "index", "video", "videos", "embed", "player", "play", "v", "view",
    "file", "files", "download", "downloads", "get", "media", "stream", "page",
    "default", "detail", "item", "content", "movie", "show", "episode", "live",
];

/// Whether a file name's stem is a generic route rather than a real name.
pub fn is_generic_stem(name: &str) -> bool {
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    let stem = stem.trim().to_ascii_lowercase();
    stem.is_empty() || GENERIC_STEMS.contains(&stem.as_str())
}

/// The extension of a file name, lowercased (without the dot).
pub fn extension_of(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.trim().to_ascii_lowercase())
        .filter(|ext| !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Locate an executable: the usual system directories first, then `PATH`.
/// A desktop launch has a minimal `PATH`, so the fixed directories matter.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = ["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin", "/opt/homebrew/bin"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Default download directory: `$HOME/Downloads`, else the current directory.
pub fn default_download_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let downloads = PathBuf::from(home).join("Downloads");
        if downloads.is_dir() {
            return downloads;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Sidecar metadata path for a partially-downloaded file (`foo.zip` ->
/// `foo.zip.rdm.json`).
pub fn sidecar_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| "download".into());
    name.push(".rdm.json");
    output.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_path_separators() {
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_filename("  ..hidden.. "), "hidden");
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("???"), "___");
    }

    #[test]
    fn filename_from_url_handles_query_and_encoding() {
        assert_eq!(
            filename_from_url("https://example.com/files/report%20final.pdf?token=1"),
            "report final.pdf"
        );
        assert_eq!(filename_from_url("https://example.com/"), "download");
        assert_eq!(filename_from_url("https://example.com/a/b/"), "b");
    }

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn unique_path_appends_counter() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "file.txt");
        assert_eq!(first.file_name().unwrap(), "file.txt");
        std::fs::write(&first, b"x").unwrap();
        let second = unique_path(dir.path(), "file.txt");
        assert_eq!(second.file_name().unwrap(), "file (1).txt");
    }

    #[test]
    fn sidecar_name() {
        let p = sidecar_path(Path::new("/tmp/foo.tar.gz"));
        assert_eq!(p.file_name().unwrap(), "foo.tar.gz.rdm.json");
    }

    #[test]
    fn generic_route_stems_are_recognised() {
        // The `watch?v=…` case: the URL says nothing about the file.
        assert!(is_generic_stem("watch"));
        assert!(is_generic_stem("watch.mp4"));
        assert!(is_generic_stem("index.html"));
        assert!(is_generic_stem(""));
        assert!(!is_generic_stem("interview-1080p.mp4"));
        assert!(!is_generic_stem("ubuntu-26.04.iso"));
        assert!(!is_generic_stem("report"));

        assert_eq!(extension_of("clip.mp4").as_deref(), Some("mp4"));
        assert_eq!(extension_of("archive.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of("watch?v=1").as_deref(), None);
        assert_eq!(extension_of("noextension").as_deref(), None);
    }
}
