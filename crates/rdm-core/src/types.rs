use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Name of the queue every download falls into unless it names another.
pub const DEFAULT_QUEUE: &str = "Main";

/// Lifecycle of a single transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Probing,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub fn is_active(self) -> bool {
        matches!(self, DownloadStatus::Probing | DownloadStatus::Downloading)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        )
    }
}

/// IDM-style category bucket, derived from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Compressed,
    Documents,
    Music,
    Programs,
    Video,
    Images,
    Other,
}

impl Category {
    pub fn from_filename(name: &str) -> Self {
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" | "iso" => {
                Category::Compressed
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "epub"
            | "csv" => Category::Documents,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" | "wma" => Category::Music,
            "exe" | "msi" | "deb" | "rpm" | "appimage" | "apk" | "dmg" | "flatpak" => {
                Category::Programs
            }
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" => {
                Category::Video
            }
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" => {
                Category::Images
            }
            _ => Category::Other,
        }
    }

    /// Folder name used when downloads are organised by category.
    pub fn folder(self) -> &'static str {
        match self {
            Category::Compressed => "Compressed",
            Category::Documents => "Documents",
            Category::Music => "Music",
            Category::Programs => "Programs",
            Category::Video => "Video",
            Category::Images => "Images",
            Category::Other => "Other",
        }
    }
}

/// What the server told us about a URL before we start transferring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub url: String,
    pub final_url: String,
    pub file_name: String,
    pub content_type: Option<String>,
    pub total_size: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    Pending,
    Active,
    Done,
    Failed,
}

/// One contiguous byte range of a file. `end` is inclusive, matching HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub status: SegmentStatus,
}

impl Segment {
    /// Total number of bytes this segment is responsible for.
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_done(&self) -> bool {
        self.downloaded >= self.len()
    }

    pub fn remaining(&self) -> u64 {
        self.len().saturating_sub(self.downloaded)
    }

    /// The next HTTP range to request, or `None` when finished.
    pub fn next_range(&self) -> Option<(u64, u64)> {
        if self.is_done() {
            None
        } else {
            Some((self.start + self.downloaded, self.end))
        }
    }
}

/// How a URL should be fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DownloadKind {
    /// A plain file, fetched with range requests and segmented.
    #[default]
    File,
    /// An HLS (`.m3u8`) stream, assembled from its segment list.
    Hls,
    /// A video page resolved and downloaded by `yt-dlp`.
    Ytdlp,
}

/// Everything needed to start a transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSpec {
    pub url: String,
    pub output_dir: PathBuf,
    /// Overrides the server-suggested file name when set.
    pub file_name: Option<String>,
    /// Maximum number of parallel connections (segments).
    pub connections: u8,
    /// Extra URLs that mirror the same file, tried when the primary stalls.
    pub mirrors: Vec<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub cookies: Option<String>,
    /// Named queue this download belongs to (IDM-style).
    #[serde(default)]
    pub queue: String,
    /// This download's own speed cap, in bytes/second (0 = unlimited). Applied
    /// on top of the app-wide limit.
    #[serde(default)]
    pub speed_limit: u64,
    /// Put the file in a per-category subfolder (decided after the probe).
    #[serde(default)]
    pub organize: bool,
    /// How to fetch the URL (plain file vs. an HLS stream).
    #[serde(default)]
    pub kind: DownloadKind,
    /// Optional `yt-dlp` format selector (e.g. `bv*+ba/b`); `None` = best.
    #[serde(default)]
    pub format: Option<String>,
    /// Preferred maximum video height for `yt-dlp` (e.g. `720`); `None` = best.
    #[serde(default)]
    pub max_height: Option<u32>,
    /// The media type the browser observed for this URL, when it reported one.
    /// Lets a sniffed stream be routed without guessing from the URL.
    #[serde(default)]
    pub content_type: Option<String>,
    /// Queue the download without starting it, so it waits for the user or the
    /// scheduler (IDM's "Download Later").
    #[serde(default)]
    pub start_paused: bool,
    /// What the page says this file is called — `og:title`, JSON-LD, `<title>`.
    /// Used to name the file when the URL's own stem is useless (`watch`).
    #[serde(default)]
    pub title: Option<String>,
}

impl DownloadSpec {
    pub fn new(url: impl Into<String>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            output_dir: output_dir.into(),
            file_name: None,
            connections: 8,
            mirrors: Vec::new(),
            user_agent: None,
            referer: None,
            cookies: None,
            queue: DEFAULT_QUEUE.to_string(),
            speed_limit: 0,
            organize: false,
            kind: DownloadKind::File,
            format: None,
            max_height: None,
            content_type: None,
            start_paused: false,
            title: None,
        }
    }

    /// Resolve and download this URL with `yt-dlp`.
    pub fn ytdlp(mut self) -> Self {
        self.kind = DownloadKind::Ytdlp;
        self
    }

    /// Ask for the best video no taller than `height` (0 = best available).
    pub fn max_height(mut self, height: u32) -> Self {
        self.max_height = (height > 0).then_some(height);
        self
    }

    /// Record the media type the browser observed (`video/mp4`,
    /// `application/vnd.apple.mpegurl`, `application/dash+xml`, …).
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        let value = content_type.into();
        self.content_type = (!value.trim().is_empty()).then_some(value);
        self
    }

    /// Add this download *without* starting it (IDM's "Download Later"): it
    /// waits in the queue until it is resumed or the scheduler window opens.
    pub fn start_paused(mut self) -> Self {
        self.start_paused = true;
        self
    }

    /// Name the download after what the page calls it.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        let value = title.into();
        self.title = (!value.trim().is_empty()).then_some(value);
        self
    }

    /// Fetch this URL as an HLS stream and assemble the segments.
    pub fn hls(mut self) -> Self {
        self.kind = DownloadKind::Hls;
        self
    }

    /// The queue name, treating an empty value as the default queue.
    pub fn queue_name(&self) -> &str {
        if self.queue.is_empty() {
            DEFAULT_QUEUE
        } else {
            &self.queue
        }
    }

    pub fn connections(mut self, n: u8) -> Self {
        self.connections = n.max(1);
        self
    }

    /// Cap this download's throughput (bytes/second, 0 = unlimited).
    pub fn speed_limit(mut self, bytes_per_sec: u64) -> Self {
        self.speed_limit = bytes_per_sec;
        self
    }

    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }
}

/// The persisted, observable state of a transfer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub spec: DownloadSpec,
    pub status: DownloadStatus,
    pub remote: Option<RemoteInfo>,
    pub segments: Vec<Segment>,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Smoothed throughput in bytes/second.
    pub speed: f64,
    pub eta_secs: Option<u64>,
    pub category: Category,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl DownloadTask {
    /// Fraction complete in `0.0..=1.0`, or `None` when the size is unknown.
    pub fn progress(&self) -> Option<f64> {
        self.total.filter(|t| *t > 0).map(|t| {
            (self.downloaded as f64 / t as f64).clamp(0.0, 1.0)
        })
    }

    pub fn output_path(&self) -> PathBuf {
        let name = self
            .spec
            .file_name
            .clone()
            .or_else(|| self.remote.as_ref().map(|r| r.file_name.clone()))
            .unwrap_or_else(|| "download".to_string());
        self.spec.output_dir.join(name)
    }
}

/// A lightweight, cheap-to-clone snapshot pushed to the UI on every tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub id: String,
    pub status: DownloadStatus,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: f64,
    pub eta_secs: Option<u64>,
    pub segments: Vec<Segment>,
}

impl ProgressUpdate {
    pub fn from_task(task: &DownloadTask) -> Self {
        Self {
            id: task.id.clone(),
            status: task.status,
            downloaded: task.downloaded,
            total: task.total,
            speed: task.speed,
            eta_secs: task.eta_secs,
            segments: task.segments.clone(),
        }
    }
}
