//! The async orchestrator: turns a [`DownloadSpec`] into a running, observable
//! transfer and exposes pause/resume/cancel controls.
//!
//! Concurrency is bounded by a [`Semaphore`] so the engine never opens more
//! parallel *downloads* than configured, while each download fans out into up
//! to `connections` parallel *segments*.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::stream::{FuturesUnordered, StreamExt};
use reqwest::{Client, Response};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{broadcast, Semaphore};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use url::Url;

use crate::error::{Error, Result};
use crate::grabber::{self, Found, FoundKind, GrabOptions};
use crate::http::{self, RequestOptions};
use crate::rate::RateLimiter;
use crate::media;
use crate::segment;
use crate::speed::{eta_secs, SpeedMeter};
use crate::store;
use crate::types::{
    Category, DownloadKind, DownloadSpec, DownloadStatus, DownloadTask, ProgressUpdate, RemoteInfo,
    Segment, SegmentStatus,
};
use crate::util;
use crate::ytdlp;

/// How often the engine emits progress snapshots and flushes to disk.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// How much of the file the opening request asks for. The response that tells us
/// the size also carries these bytes, so the transfer starts inside the round
/// trip that used to be spent probing (see [`http::probe_streaming`]).
const FIRST_CHUNK: u64 = 4 * 1024 * 1024;

/// Tunables for the manager.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of transfers running at once; the rest queue up.
    pub max_concurrent: usize,
    /// Default connection count for specs that do not specify one.
    pub default_connections: u8,
    /// Per-queue concurrency limits; queues not listed use `default_connections`.
    pub queue_limits: HashMap<String, usize>,
    /// While a transfer runs, split the largest remaining segment to use a freed
    /// connection (IDM-style dynamic segmentation). Needs range support.
    pub dynamic_segmentation: bool,
    /// Never split a segment smaller than this many bytes.
    pub min_segment_size: u64,
    /// Browser whose cookie store `yt-dlp` should read (e.g. `chrome`).
    pub ytdlp_cookies_from_browser: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            default_connections: 8,
            queue_limits: HashMap::new(),
            dynamic_segmentation: true,
            min_segment_size: 1 << 20, // 1 MiB
            ytdlp_cookies_from_browser: None,
        }
    }
}

/// Everything a UI needs to react to, pushed over a broadcast channel.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { task: DownloadTask },
    Progress { update: ProgressUpdate },
    StateChanged { task: DownloadTask },
    Removed { id: String },
}

#[derive(Clone)]
struct Control {
    cancel: CancellationToken,
    paused: Arc<AtomicBool>,
    /// This download's own speed cap (bytes/second, 0 = unlimited).
    limiter: Arc<RateLimiter>,
}

impl Control {
    fn new(limit_bps: u64) -> Self {
        Self {
            cancel: CancellationToken::new(),
            paused: Arc::new(AtomicBool::new(false)),
            limiter: Arc::new(RateLimiter::new(limit_bps)),
        }
    }
}

/// The public handle to the engine. Cheap to clone via `Arc`.
pub struct DownloadManager {
    config: EngineConfig,
    client: Client,
    tasks: Arc<RwLock<BTreeMap<String, DownloadTask>>>,
    controls: Arc<Mutex<HashMap<String, Control>>>,
    semaphore: Arc<Semaphore>,
    events: broadcast::Sender<EngineEvent>,
    limiter: Arc<RateLimiter>,
    /// One semaphore per named queue, bounding that queue's concurrency.
    queues: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
    /// Queue limits, so `set_queue_limit` can compute permit deltas.
    queue_limits: Arc<RwLock<HashMap<String, usize>>>,
    /// Queues the user has stopped; tasks in them wait.
    stopped_queues: Arc<RwLock<std::collections::HashSet<String>>>,
    queue_notify: Arc<tokio::sync::Notify>,
    /// Bodies already open for a task: the opening request asked for the file's
    /// first chunk, and those bytes are the first segment's data — no point
    /// asking the server twice. The flag is whether ranges are supported, which
    /// decides how much of the body may be used.
    primed: Arc<Mutex<HashMap<String, (reqwest::Response, bool)>>>,
}

impl DownloadManager {
    pub fn new(config: EngineConfig) -> Result<Arc<Self>> {
        let (events, _) = broadcast::channel(2048);
        let queue_limits = config.queue_limits.clone();
        Ok(Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent.max(1))),
            client: http::build_client()?,
            config,
            tasks: Arc::new(RwLock::new(BTreeMap::new())),
            controls: Arc::new(Mutex::new(HashMap::new())),
            events,
            limiter: Arc::new(RateLimiter::new(0)),
            queues: Arc::new(RwLock::new(HashMap::new())),
            queue_limits: Arc::new(RwLock::new(queue_limits)),
            stopped_queues: Arc::new(RwLock::new(std::collections::HashSet::new())),
            queue_notify: Arc::new(tokio::sync::Notify::new()),
            primed: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    /// The browser yt-dlp should read cookies from: whatever the user chose,
    /// else the first browser that is installed. Sites like YouTube refuse
    /// anonymous extraction outright ("confirm you're not a bot", or an
    /// extractor crash), and the signed-in session already lives in that browser.
    fn cookie_browser(&self) -> Option<String> {
        self.config
            .ytdlp_cookies_from_browser
            .clone()
            .filter(|name| !name.trim().is_empty())
            .or_else(ytdlp::detect_browser)
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Set the application-wide speed limit in bytes/second (0 = unlimited).
    pub fn set_speed_limit(&self, bytes_per_sec: u64) {
        self.limiter.set_limit(bytes_per_sec);
    }

    /// Set a single download's own speed cap in bytes/second (0 = unlimited).
    pub async fn set_download_speed_limit(&self, id: &str, bytes_per_sec: u64) -> Result<()> {
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;
        control.limiter.set_limit(bytes_per_sec);
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.spec.speed_limit = bytes_per_sec;
        }
        Ok(())
    }

    /// Re-insert a previously persisted task, so a restart restores the queue.
    /// Unfinished transfers come back paused; the caller resumes them.
    pub async fn restore(&self, mut task: DownloadTask) -> Result<String> {
        if !task.status.is_terminal() {
            task.status = DownloadStatus::Paused;
        }
        task.speed = 0.0;
        task.eta_secs = None;
        for seg in &mut task.segments {
            if !seg.is_done() {
                seg.status = SegmentStatus::Pending;
            }
        }
        let id = task.id.clone();
        self.tasks.write().await.insert(id.clone(), task.clone());
        self.controls
            .lock()
            .await
            .insert(id.clone(), Control::new(task.spec.speed_limit));
        let _ = self.events.send(EngineEvent::Added { task });
        Ok(id)
    }

    // ---- named queues ----------------------------------------------------

    /// How many transfers may run in `queue` at once.
    pub async fn queue_limit(&self, queue: &str) -> usize {
        self.queue_limits
            .read()
            .await
            .get(queue)
            .copied()
            .unwrap_or(self.config.default_connections.max(1) as usize)
    }

    /// Change a queue's concurrency; waiting tasks pick it up immediately.
    pub async fn set_queue_limit(&self, queue: &str, limit: usize) {
        let limit = limit.max(1);
        let semaphore = {
            let mut queues = self.queues.write().await;
            queues
                .entry(queue.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(limit)))
                .clone()
        };
        let mut limits = self.queue_limits.write().await;
        let old = limits.insert(queue.to_string(), limit).unwrap_or(limit);
        if limit > old {
            semaphore.add_permits(limit - old);
        } else if limit < old {
            // Best effort: only permits that are currently free can be removed.
            let forgotten = semaphore.forget_permits(old - limit);
            if forgotten < old - limit {
                limits.insert(queue.to_string(), old - forgotten);
            }
        }
        drop(limits);
        self.queue_notify.notify_waiters();
    }

    /// Names of every queue the engine knows about, sorted.
    pub async fn queue_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.queue_limits.read().await.keys().cloned().collect();
        names.sort();
        names
    }

    pub async fn is_queue_stopped(&self, queue: &str) -> bool {
        self.stopped_queues.read().await.contains(queue)
    }

    /// Stop a queue: running tasks are paused, new ones wait.
    pub async fn stop_queue(&self, queue: &str) {
        self.stopped_queues
            .write()
            .await
            .insert(queue.to_string());
        let ids: Vec<String> = self
            .tasks
            .read()
            .await
            .values()
            .filter(|task| task.spec.queue_name() == queue)
            .map(|task| task.id.clone())
            .collect();
        for id in ids {
            let _ = self.pause(&id).await;
        }
    }

    /// Start a queue again and resume the tasks it paused.
    pub async fn start_queue(self: &Arc<Self>, queue: &str) {
        self.stopped_queues.write().await.remove(queue);
        self.queue_notify.notify_waiters();
        let ids: Vec<String> = self
            .tasks
            .read()
            .await
            .values()
            .filter(|task| {
                task.spec.queue_name() == queue && task.status == DownloadStatus::Paused
            })
            .map(|task| task.id.clone())
            .collect();
        for id in ids {
            let _ = self.resume(&id).await;
        }
    }

    /// Wait until `queue` is started, then take one of its concurrency slots.
    async fn acquire_queue_slot(&self, queue: &str) -> tokio::sync::OwnedSemaphorePermit {
        loop {
            if !self.stopped_queues.read().await.contains(queue) {
                break;
            }
            self.queue_notify.notified().await;
        }
        let semaphore = {
            let limit = self.queue_limit(queue).await;
            let mut queues = self.queues.write().await;
            queues
                .entry(queue.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(limit)))
                .clone()
        };
        semaphore
            .acquire_owned()
            .await
            .expect("queue semaphore closed")
    }

    /// Subscribe to engine events (progress ticks, state changes).
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    pub async fn list(&self) -> Vec<DownloadTask> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<DownloadTask> {
        self.tasks.read().await.get(id).cloned()
    }

    /// Inspect a URL without starting a transfer.
    pub async fn probe(&self, spec: &DownloadSpec) -> Result<RemoteInfo> {
        let opts = RequestOptions::from_spec(spec);
        http::probe(&self.client, &spec.url, &opts).await
    }

    /// The video heights a URL offers, largest first, for a quality picker.
    /// Runs `yt-dlp -J`; empty when the tool is missing or the URL has no video.
    /// The caller's cookies are forwarded, since the same sites that block a
    /// download also refuse to list formats anonymously.
    pub async fn qualities(
        &self,
        url: &str,
        cookies: Option<&str>,
        user_agent: Option<&str>,
        referer: Option<&str>,
    ) -> Result<Vec<u32>> {
        let binary =
            ytdlp::find_binary().ok_or_else(|| Error::invalid("yt-dlp is not installed"))?;
        let js_runtime = ytdlp::find_js_runtime();
        let browser = self.cookie_browser();
        let opts = ytdlp::Options {
            cookies: cookies.filter(|value| !value.trim().is_empty()),
            user_agent,
            referer,
            cookies_from_browser: browser.as_deref(),
            js_runtime: js_runtime.as_deref(),
            ..ytdlp::Options::default()
        };

        let output = tokio::process::Command::new(&binary)
            .args(ytdlp::list_args(url, &opts))
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|err| Error::internal(format!("could not run yt-dlp: {err}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::internal(ytdlp_failure(
                &stderr,
                "yt-dlp could not list formats",
            )));
        }
        Ok(ytdlp::parse_heights(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Crawl a site and collect the links worth downloading (site grabber).
    /// Pages are followed up to `opts.depth` deep. With `opts.same_origin` (the
    /// default) both crawling *and* collection stay on the start URL's origin,
    /// so a grab never wanders off-site; turn it off to include CDNs.
    pub async fn grab_site(&self, start: &str, opts: GrabOptions) -> Result<Vec<Found>> {
        let base = Url::parse(start).map_err(|_| Error::invalid("that is not a valid URL"))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(Error::invalid("only http(s) URLs are supported"));
        }
        let origin = base.origin();
        let request = RequestOptions::default();

        let mut queue: VecDeque<(Url, u8)> = VecDeque::from([(base, 0)]);
        let mut seen: HashSet<String> = HashSet::new();
        let mut found: Vec<Found> = Vec::new();

        while let Some((page, depth)) = queue.pop_front() {
            if found.len() >= opts.limit {
                break;
            }
            if !seen.insert(page.as_str().to_string()) {
                continue;
            }
            // A page that will not load is skipped, not fatal: grabs run over
            // whole sites and dead links are normal.
            let Ok(html) = http::fetch_text(&self.client, page.as_str(), &request).await else {
                continue;
            };
            for link in grabber::extract_links(&html, &page) {
                let kind = grabber::classify(&link);
                let on_origin = link.origin() == origin;
                if opts.same_origin && !on_origin {
                    continue;
                }
                match kind {
                    FoundKind::Page => {
                        // A page is marked seen when it is *processed*, never
                        // when it is discovered — otherwise queuing it would
                        // also skip it.
                        if depth < opts.depth {
                            queue.push_back((link, depth + 1));
                        }
                    }
                    _ => {
                        if seen.insert(link.as_str().to_string()) {
                            found.push(Found {
                                url: link.as_str().to_string(),
                                kind,
                            });
                        }
                    }
                }
            }
        }

        tracing::info!(%start, found = found.len(), "site grab finished");
        Ok(grabber::select(&found, &opts))
    }

    /// Queue and start a transfer. Returns the new id *immediately* — the probe
    /// and segment plan happen in the background, so the UI never stalls on a
    /// slow server.
    pub async fn add(self: &Arc<Self>, spec: DownloadSpec) -> Result<String> {
        let mut spec = spec;
        if !(spec.url.starts_with("http://") || spec.url.starts_with("https://")) {
            return Err(Error::invalid("only http(s) URLs are supported"));
        }
        let dir = spec.output_dir.clone();
        tokio::fs::create_dir_all(&dir).await?;

        // Duplicate detection: the same URL already queued (from the UI, the
        // clipboard monitor or the browser) returns the existing id rather than
        // creating another row.
        if let Some(existing) = self
            .tasks
            .read()
            .await
            .values()
            .find(|task| task.spec.url == spec.url)
        {
            return Ok(existing.id.clone());
        }

        // Auto-detect HLS by extension so every entry point (UI, clipboard, IPC)
        // benefits without the caller having to say so.
        if spec.kind == DownloadKind::File && media::is_hls_url(&spec.url) {
            spec.kind = DownloadKind::Hls;
        }
        // A DASH manifest describes the streams rather than being one, and
        // yt-dlp assembles it — never save the XML as if it were the video.
        if spec.kind == DownloadKind::File && media::is_dash_url(&spec.url) {
            spec.kind = DownloadKind::Ytdlp;
        }

        // A provisional name from the URL gives the row something to show until
        // the probe refines it (unless the caller named the file).
        if spec.file_name.is_none() {
            spec.file_name = Some(provisional_name(&spec));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let category = spec
            .file_name
            .as_deref()
            .map(Category::from_filename)
            .unwrap_or(Category::Other);
        let speed_limit = spec.speed_limit;
        // "Download Later": the task exists (and is visible) but nothing
        // transfers until it is resumed.
        let start_paused = spec.start_paused;
        let task = DownloadTask {
            id: id.clone(),
            spec,
            status: if start_paused {
                DownloadStatus::Paused
            } else {
                DownloadStatus::Queued
            },
            total: None,
            remote: None,
            segments: Vec::new(),
            downloaded: 0,
            speed: 0.0,
            eta_secs: None,
            category,
            error: None,
            created_at: now(),
            completed_at: None,
        };

        self.tasks.write().await.insert(id.clone(), task.clone());
        self.controls
            .lock()
            .await
            .insert(id.clone(), Control::new(speed_limit));
        let _ = self.events.send(EngineEvent::Added { task });
        if !start_paused {
            self.spawn(id.clone());
        }
        Ok(id)
    }

    /// Probe the URL and lay out the segment plan; decides the final file name
    /// and, when requested, the per-category folder. Runs inside the transfer
    /// task so [`add`](Self::add) stays instant.
    async fn probe_and_plan(&self, id: &str) -> Result<()> {
        let spec = {
            let tasks = self.tasks.read().await;
            match tasks.get(id) {
                Some(task) => task.spec.clone(),
                None => return Ok(()),
            }
        };
        self.set_status(id, DownloadStatus::Probing).await;

        let opts = RequestOptions::from_spec(&spec);
        // Probe and start in one request: the body we get back is the beginning
        // of the file, so the first segment is already on its way.
        let (remote, primed) =
            http::probe_streaming(&self.client, &spec.url, &opts, FIRST_CHUNK).await?;
        let accept_ranges = remote.accept_ranges;
        tracing::info!(
            url = %spec.url,
            size = ?remote.total_size,
            ranges = remote.accept_ranges,
            "probe ok"
        );

        // A web page (or a DASH manifest) is not a file: handing it to the file
        // downloader would save the HTML/PHP/XML. Re-route to yt-dlp, which
        // assembles the streams, or fail with a clear message.
        //
        // The browser's observation wins when it has one: the extension reports
        // the type of the request the *player* made, which is more reliable than
        // probing a URL that may be signed, expiring, or extension-less.
        let content_type = remote.content_type.clone().unwrap_or_default();
        let hinted = spec
            .content_type
            .as_deref()
            .and_then(media::stream_kind);
        let is_html = content_type.starts_with("text/html") || content_type.contains("xhtml");
        let is_dash = hinted == Some(DownloadKind::Ytdlp)
            || media::is_dash_content_type(&content_type);
        // An HLS playlist served without a `.m3u8` name still needs assembling.
        let is_hls = spec.kind != DownloadKind::Hls
            && (hinted == Some(DownloadKind::Hls)
                || media::is_hls_content_type(&content_type));
        if (is_html || is_dash) && !ytdlp::is_available() {
            return Err(Error::invalid(
                "that URL is a web page or a streaming manifest, not a media file — install yt-dlp to download the video",
            ));
        }

        let requested = if spec.connections == 0 {
            self.config.default_connections
        } else {
            spec.connections
        };
        let segments = match remote.total_size {
            Some(total) if remote.accept_ranges => segment::plan(total, requested),
            Some(total) => segment::plan(total, 1),
            None => Vec::new(),
        };

        // Resolve the final name/dir before taking the lock (no await inside it).
        let provisional = util::filename_from_url(&spec.url);
        let server_name = util::sanitize_filename(&remote.file_name);
        let chosen = if !server_name.is_empty()
            && !util::is_generic_stem(&server_name)
            && spec.file_name.as_deref() == Some(provisional.as_str())
        {
            server_name
        } else {
            spec.file_name.clone().unwrap_or(provisional)
        };
        // A playlist is assembled into an MPEG-TS file, not saved as a playlist.
        let chosen = if is_hls {
            format!("{}.ts", chosen.trim_end_matches(".m3u8"))
        } else {
            chosen
        };
        let unique = util::unique_path(&spec.output_dir, &chosen);
        let final_name = unique
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or(chosen);
        let category = Category::from_filename(&final_name);
        let mut output_dir = spec.output_dir.clone();
        if spec.organize {
            let target = output_dir.join(category.folder());
            let _ = tokio::fs::create_dir_all(&target).await;
            output_dir = target;
        }

        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            if is_html || is_dash {
                task.spec.kind = DownloadKind::Ytdlp;
            } else if is_hls {
                task.spec.kind = DownloadKind::Hls;
            }
            task.spec.file_name = Some(final_name);
            task.spec.output_dir = output_dir;
            task.category = category;
            task.total = remote.total_size;
            task.remote = Some(remote);
            task.segments = segments;
        }
        let snapshot = tasks.get(id).cloned();
        drop(tasks);
        if let Some(task) = snapshot {
            let _ = self.events.send(EngineEvent::StateChanged { task });
        }

        // Hand the opening body to the transfer. The page and manifest cases are
        // routed elsewhere, so their bodies are dropped here.
        if let Some(response) = primed {
            if !is_html && !is_dash && !is_hls {
                self.primed
                    .lock()
                    .await
                    .insert(id.to_string(), (response, accept_ranges));
            }
        }
        Ok(())
    }

    /// Ask a running transfer to stop; it will settle into `Paused`.
    pub async fn pause(&self, id: &str) -> Result<()> {
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;
        control.paused.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Continue a paused/failed/cancelled transfer from where it stopped.
    pub async fn resume(self: &Arc<Self>, id: &str) -> Result<()> {
        {
            let tasks = self.tasks.read().await;
            let task = tasks.get(id).ok_or_else(|| not_found(id))?;
            if !matches!(
                task.status,
                DownloadStatus::Paused | DownloadStatus::Failed | DownloadStatus::Cancelled
            ) {
                return Err(Error::invalid("only paused, failed or cancelled transfers can resume"));
            }
        }
        let speed_limit = {
            let tasks = self.tasks.read().await;
            tasks.get(id).map(|task| task.spec.speed_limit).unwrap_or(0)
        };
        self.controls
            .lock()
            .await
            .insert(id.to_string(), Control::new(speed_limit));
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(id) {
                task.status = DownloadStatus::Queued;
                task.error = None;
                for seg in &mut task.segments {
                    if !seg.is_done() {
                        seg.status = SegmentStatus::Pending;
                    }
                }
            }
        }
        self.spawn(id.to_string());
        Ok(())
    }

    /// Abort a transfer outright.
    pub async fn cancel(&self, id: &str) -> Result<()> {
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;
        control.cancel.cancel();
        Ok(())
    }

    /// Forget a transfer, optionally deleting the partial file on disk.
    pub async fn remove(self: &Arc<Self>, id: &str, delete_file: bool) -> Result<()> {
        if let Some(control) = self.control(id).await {
            control.cancel.cancel();
        }
        let task = self.tasks.write().await.remove(id);
        if let Some(task) = task {
            let _ = store::remove(&task);
            if delete_file {
                let _ = tokio::fs::remove_file(task.output_path()).await;
            }
        }
        self.controls.lock().await.remove(id);
        let _ = self.events.send(EngineEvent::Removed { id: id.to_string() });
        Ok(())
    }

    // ---- internals -------------------------------------------------------

    fn spawn(self: &Arc<Self>, id: String) {
        let this = self.clone();
        tokio::spawn(async move { this.run(id).await });
    }

    async fn control(&self, id: &str) -> Option<Control> {
        self.controls.lock().await.get(id).cloned()
    }

    async fn set_status(&self, id: &str, status: DownloadStatus) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            task.status = status;
        }
    }

    async fn run(self: Arc<Self>, id: String) {
        // Wait for a slot in this download's named queue, then a global slot.
        let queue = {
            let tasks = self.tasks.read().await;
            match tasks.get(&id) {
                Some(task) => task.spec.queue_name().to_string(),
                None => return,
            }
        };
        let queue_permit = self.acquire_queue_slot(&queue).await;
        let Ok(permit) = self.semaphore.clone().acquire_owned().await else {
            return;
        };
        if self
            .control(&id)
            .await
            .is_none_or(|c| c.cancel.is_cancelled())
        {
            return;
        }

        // Probe/resolve here (not in `add`) so the UI is never blocked on the
        // network.
        let kind = {
            let tasks = self.tasks.read().await;
            tasks
                .get(&id)
                .map(|task| task.spec.kind)
                .unwrap_or(DownloadKind::File)
        };

        let outcome = match kind {
            DownloadKind::Ytdlp => {
                self.set_status(&id, DownloadStatus::Downloading).await;
                self.transfer_ytdlp(&id).await
            }
            DownloadKind::Hls => {
                self.set_status(&id, DownloadStatus::Probing).await;
                self.transfer_hls(&id).await
            }
            DownloadKind::File => {
                let needs_probe = {
                    let tasks = self.tasks.read().await;
                    tasks.get(&id).map(|task| task.remote.is_none()).unwrap_or(false)
                };
                if needs_probe {
                    if let Err(err) = self.probe_and_plan(&id).await {
                        drop(permit);
                        drop(queue_permit);
                        self.finish(&id, DownloadStatus::Failed, Some(err.to_string()))
                            .await;
                        return;
                    }
                }
                // A page that turned out to be HTML is re-routed to yt-dlp by
                // `probe_and_plan` (or rejected there); follow that here.
                let effective = {
                    let tasks = self.tasks.read().await;
                    tasks
                        .get(&id)
                        .map(|task| task.spec.kind)
                        .unwrap_or(DownloadKind::File)
                };
                self.set_status(&id, DownloadStatus::Downloading).await;
                if effective == DownloadKind::Ytdlp {
                    self.transfer_ytdlp(&id).await
                } else {
                    self.transfer(&id).await
                }
            }
        };
        drop(permit);
        drop(queue_permit);

        let (status, error) = match outcome {
            Ok(()) => (DownloadStatus::Completed, None),
            Err(Error::Paused) => (DownloadStatus::Paused, None),
            Err(Error::Cancelled) => (DownloadStatus::Cancelled, None),
            Err(err) => {
                debug!(%id, %err, "transfer failed");
                (DownloadStatus::Failed, Some(err.to_string()))
            }
        };
        self.finish(&id, status, error).await;
    }

    async fn transfer(self: &Arc<Self>, id: &str) -> Result<()> {
        let (spec, mut segments, total, out_path) = {
            let tasks = self.tasks.read().await;
            let task = tasks.get(id).ok_or_else(|| not_found(id))?;
            (
                task.spec.clone(),
                task.segments.clone(),
                task.total,
                task.output_path(),
            )
        };
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;
        let opts = RequestOptions::from_spec(&spec);

        if let Some(parent) = out_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        match total {
            Some(total) if total > 0 => preallocate(&out_path, total).await?,
            _ => {
                tokio::fs::File::create(&out_path).await?;
            }
        }

        // The bytes the opening request already fetched. Writing them now means
        // data lands on disk before the first segment request is even made.
        let mut primed = self.primed.lock().await.remove(id);
        if !segments.is_empty() {
            if let Some((response, ranged)) = primed.take() {
                if let Some(first) = segments.first_mut() {
                    // Ranged: the body is capped at the opening chunk, so take
                    // what is there and let the worker finish the segment. Not
                    // ranged: the body is the whole file, so take all of it.
                    let want = if ranged {
                        first.len().min(FIRST_CHUNK)
                    } else {
                        first.len()
                    };
                    match write_prefix(response, &out_path, first.start, want, &self.limiter, &control)
                        .await
                    {
                        Ok(written) if written > 0 => {
                            first.downloaded = written;
                            if written >= first.len() {
                                first.status = SegmentStatus::Done;
                            }
                            tracing::info!(%id, bytes = written, "started from the opening request");
                        }
                        Ok(_) => {}
                        Err(err) => {
                            // Not fatal: the segment worker will fetch it instead.
                            tracing::warn!(%id, %err, "could not use the opening body");
                        }
                    }
                }
            }
        }

        let lives = Arc::new(StdMutex::new(build_lives(&segments)));

        // The task list should show the bytes we already have.
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(id) {
                task.segments = segments.clone();
            }
        }

        let single = Arc::new(AtomicU64::new(0));

        let ticker = self.spawn_ticker(id.to_string(), lives.clone(), single.clone(), total);

        let result = if segments.is_empty() {
            self.stream_single(
                &spec,
                &out_path,
                &opts,
                &single,
                &self.limiter,
                &control,
                primed.map(|(response, _)| response),
            )
            .await
        } else {
            self.stream_segments(&spec, &lives, &out_path, &self.limiter, &control)
                .await
        };

        ticker.abort();
        self.sync_progress(id, &lives, &single).await;
        result
    }

    /// Assemble an HLS stream: resolve the playlist, then fetch and concatenate
    /// any init segment and the ordered segments into the output file.
    async fn transfer_hls(self: &Arc<Self>, id: &str) -> Result<()> {
        let (spec, out_path) = {
            let tasks = self.tasks.read().await;
            let task = tasks.get(id).ok_or_else(|| not_found(id))?;
            (task.spec.clone(), task.output_path())
        };
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;
        let opts = RequestOptions::from_spec(&spec);

        if let Some(parent) = out_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let single = Arc::new(AtomicU64::new(0));
        let lives = Arc::new(StdMutex::new(Vec::new()));
        let ticker = self.spawn_ticker(id.to_string(), lives.clone(), single.clone(), None);

        let result = async {
            let playlist = self.resolve_hls(&spec.url, &opts).await?;
            if playlist.segments.is_empty() {
                return Err(Error::invalid("the HLS playlist has no segments"));
            }
            self.set_status(id, DownloadStatus::Downloading).await;

            let mut file = tokio::fs::File::create(&out_path).await?;
            let mut written = 0u64;

            let mut sources: Vec<&Url> = Vec::new();
            if let Some(init) = &playlist.init {
                sources.push(init);
            }
            sources.extend(playlist.segments.iter());

            for source in sources {
                if control.cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if control.paused.load(Ordering::Relaxed) {
                    return Err(Error::Paused);
                }
                let bytes = http::fetch_bytes(&self.client, source.as_str(), &opts).await?;
                let len = bytes.len() as u64;
                self.limiter.acquire(len).await;
                control.limiter.acquire(len).await;
                file.write_all(&bytes).await?;
                written += len;
                single.store(written, Ordering::Relaxed);
            }
            file.flush().await?;
            // Assembled HLS is MPEG-TS; copy the streams into an MP4 so the
            // result plays anywhere. Falls back to the `.ts` if ffmpeg is absent.
            if let Some(remuxed) = remux_to_mp4(&out_path).await {
                if let Some(name) = remuxed.file_name().and_then(|n| n.to_str()) {
                    let name = name.to_string();
                    let mut tasks = self.tasks.write().await;
                    if let Some(task) = tasks.get_mut(id) {
                        task.spec.file_name = Some(name);
                    }
                }
            }
            Ok(())
        }
        .await;

        ticker.abort();
        self.sync_progress(id, &lives, &single).await;
        result
    }

    /// Fetch a playlist and, if it is a master, follow the highest-bandwidth
    /// variant.
    async fn resolve_hls(&self, url: &str, opts: &RequestOptions) -> Result<media::MediaPlaylist> {
        let text = http::fetch_text(&self.client, url, opts).await?;
        let base = Url::parse(url)?;
        if media::is_master_playlist(&text) {
            let variant = media::parse_master(&text, &base)
                .into_iter()
                .max_by_key(|v| v.bandwidth)
                .ok_or_else(|| Error::invalid("no variants in the HLS master playlist"))?;
            let text = http::fetch_text(&self.client, variant.url.as_str(), opts).await?;
            Ok(media::parse_media(&text, &variant.url))
        } else {
            Ok(media::parse_media(&text, &base))
        }
    }

    /// Download a video page with `yt-dlp`, which resolves the sites a manifest
    /// parser cannot.
    async fn transfer_ytdlp(self: &Arc<Self>, id: &str) -> Result<()> {
        let (spec, out_path) = {
            let tasks = self.tasks.read().await;
            let task = tasks.get(id).ok_or_else(|| not_found(id))?;
            (task.spec.clone(), task.output_path())
        };
        let control = self.control(id).await.ok_or_else(|| not_found(id))?;

        let binary =
            ytdlp::find_binary().ok_or_else(|| Error::invalid("yt-dlp is not installed"))?;

        let dir = out_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        tokio::fs::create_dir_all(&dir).await?;

        // yt-dlp picks the container; give it a stem — unless the URL gave us
        // nothing usable and the page told us nothing, in which case yt-dlp's own
        // `%(title)s` is the right name (this is what stops `watch.mp4`).
        let stem = out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video")
            .to_string();
        // Who names the file. An extractor knows the media's real title, and a
        // derived stem is usually a route (`watch`) or an SEO-suffixed page
        // title, so yt-dlp names it unless the caller asked for a specific name.
        let named_by_title = !spec.keep_name;
        let template = if named_by_title {
            dir.join("%(title)s.%(ext)s")
        } else {
            dir.join(format!("{stem}.%(ext)s"))
        };

        let browser = self.cookie_browser();
        if let Some(browser) = &browser {
            tracing::debug!(%browser, "yt-dlp will read that browser's cookies");
        }
        let mut command = tokio::process::Command::new(&binary);
        let js_runtime = ytdlp::find_js_runtime();
        let selector = spec
            .format
            .clone()
            .unwrap_or_else(|| ytdlp::format_selector(spec.max_height));
        let opts = ytdlp::Options {
            format: Some(selector.as_str()),
            cookies: spec.cookies.as_deref(),
            user_agent: spec.user_agent.as_deref(),
            referer: spec.referer.as_deref(),
            cookies_from_browser: browser.as_deref(),
            js_runtime: js_runtime.as_deref(),
        };
        command
            .args(ytdlp::download_args(&spec.url, &template, &opts))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|err| Error::internal(format!("could not run yt-dlp: {err}")))?;

        // yt-dlp is not our transfer loop, so its progress has to be read from
        // what it prints: without this the row sits at 0% while bytes flow.
        let progress_reader = {
            let stdout = child.stdout.take();
            let tasks = self.tasks.clone();
            let events = self.events.clone();
            let task_id = id.to_string();
            tokio::spawn(async move {
                let Some(stdout) = stdout else { return };
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Some(update) = ytdlp::parse_progress(&line) else {
                        continue;
                    };
                    let mut guard = tasks.write().await;
                    if let Some(task) = guard.get_mut(&task_id) {
                        if let Some(total) = update.total {
                            task.total = Some(total);
                        }
                        if let Some(done) = update.downloaded {
                            task.downloaded = done;
                        }
                        if let Some(speed) = update.speed {
                            task.speed = speed;
                        }
                        if let Some(eta) = update.eta_secs {
                            task.eta_secs = Some(eta);
                        }
                        let _ = events.send(EngineEvent::Progress {
                            update: ProgressUpdate::from_task(task),
                        });
                    }
                }
            })
        };

        let status = loop {
            if control.cancel.is_cancelled() {
                let _ = child.kill().await;
                return Err(Error::Cancelled);
            }
            if control.paused.load(Ordering::Relaxed) {
                let _ = child.kill().await;
                return Err(Error::Paused);
            }
            match child.try_wait()? {
                Some(status) => break status,
                None => tokio::time::sleep(Duration::from_millis(300)).await,
            }
        };
        progress_reader.abort();

        if !status.success() {
            let mut stderr = String::new();
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_string(&mut stderr).await;
            }
            let detail = ytdlp_failure(&stderr, "yt-dlp failed");
            tracing::warn!(%id, "yt-dlp failed: {detail}");
            return Err(Error::internal(format!("yt-dlp failed: {detail}")));
        }

        // Record the file yt-dlp produced (it chose the extension).
        if let Some(name) = produced_file(&dir, (!named_by_title).then_some(stem.as_str())).await {
            if let Ok(meta) = tokio::fs::metadata(dir.join(&name)).await {
                let size = meta.len();
                let mut tasks = self.tasks.write().await;
                if let Some(task) = tasks.get_mut(id) {
                    task.spec.file_name = Some(name);
                    task.total = Some(size);
                    task.downloaded = size;
                }
            }
        }
        Ok(())
    }

    /// Keep up to `connections` segment workers busy. When a worker frees up and
    /// no plain work remains, split the largest remaining segment and hand the
    /// new tail to it — IDM-style dynamic segmentation.
    async fn stream_segments(
        self: &Arc<Self>,
        spec: &DownloadSpec,
        lives: &Arc<StdMutex<Vec<Live>>>,
        out_path: &Path,
        global: &Arc<RateLimiter>,
        control: &Control,
    ) -> Result<()> {
        let connections = spec.connections.max(1) as usize;
        let dynamic = self.config.dynamic_segmentation && connections > 1;
        let min_segment = self.config.min_segment_size;

        let mut running = FuturesUnordered::new();
        loop {
            while running.len() < connections {
                let Some(live) = claim_next(lives) else {
                    if dynamic && split_largest(lives, min_segment) {
                        continue;
                    }
                    break;
                };
                let client = self.client.clone();
                let spec = spec.clone();
                let out_path = out_path.to_path_buf();
                let global = global.clone();
                let control = control.clone();
                running.push(async move {
                    let result =
                        download_live(&client, &spec, &out_path, &live, &global, &control).await;
                    live.running.store(false, Ordering::Relaxed);
                    result
                });
            }

            match running.next().await {
                Some(Ok(())) => {}
                Some(Err(err)) => return Err(err),
                None => break,
            }
        }
        Ok(())
    }

    async fn stream_single(
        &self,
        spec: &DownloadSpec,
        out_path: &Path,
        opts: &RequestOptions,
        single: &Arc<AtomicU64>,
        limiter: &Arc<RateLimiter>,
        control: &Control,
        primed: Option<Response>,
    ) -> Result<()> {
        // The opening request's body is the whole file when the server would not
        // do ranges, so there is no reason to ask for it again.
        let resp = match primed {
            Some(response) => response,
            None => http::fetch_full(&self.client, &spec.url, opts).await?,
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(out_path)
            .await?;
        let mut stream = resp.bytes_stream();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            if control.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if control.paused.load(Ordering::Relaxed) {
                return Err(Error::Paused);
            }
            let chunk = chunk?;
            let len = chunk.len() as u64;
            limiter.acquire(len).await;
            control.limiter.acquire(len).await;
            file.write_all(&chunk).await?;
            written += len;
            single.store(written, Ordering::Relaxed);
        }
        file.flush().await?;
        Ok(())
    }

    fn spawn_ticker(
        &self,
        id: String,
        lives: Arc<StdMutex<Vec<Live>>>,
        single: Arc<AtomicU64>,
        total: Option<u64>,
    ) -> tokio::task::JoinHandle<()> {
        let tasks = self.tasks.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            let mut meter = SpeedMeter::default();
            let mut last = 0u64;
            let mut last_t = Instant::now();
            loop {
                tokio::time::sleep(PROGRESS_INTERVAL).await;
                let (downloaded, segments) = {
                    let guard = lives.lock().unwrap();
                    if guard.is_empty() {
                        (single.load(Ordering::Relaxed), None)
                    } else {
                        let segments = lives_to_segments(&guard);
                        let sum = segments.iter().map(|s| s.downloaded).sum();
                        (sum, Some(segments))
                    }
                };
                let elapsed = last_t.elapsed();
                meter.observe(downloaded.saturating_sub(last), elapsed);
                last = downloaded;
                last_t = Instant::now();
                let speed = meter.speed();
                let eta = total.and_then(|t| eta_secs(t.saturating_sub(downloaded), speed));

                let update = {
                    let mut tasks = tasks.write().await;
                    match tasks.get_mut(&id) {
                        Some(task) if !task.status.is_terminal() => {
                            task.downloaded = downloaded;
                            task.speed = speed;
                            task.eta_secs = eta;
                            if let Some(segments) = segments {
                                task.segments = segments;
                            }
                            Some(ProgressUpdate::from_task(task))
                        }
                        _ => None,
                    }
                };

                match update {
                    Some(update) => {
                        let _ = events.send(EngineEvent::Progress { update });
                    }
                    None => break,
                }
            }
        })
    }

    async fn sync_progress(&self, id: &str, lives: &Arc<StdMutex<Vec<Live>>>, single: &Arc<AtomicU64>) {
        let (segments, downloaded) = {
            let guard = lives.lock().unwrap();
            if guard.is_empty() {
                (None, single.load(Ordering::Relaxed))
            } else {
                let segments = lives_to_segments(&guard);
                let sum = segments.iter().map(|s| s.downloaded).sum();
                (Some(segments), sum)
            }
        };
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            if let Some(segments) = segments {
                task.segments = segments;
            }
            task.downloaded = downloaded;
            let incomplete = task.status != DownloadStatus::Completed;
            if incomplete {
                let _ = store::save(task);
            }
        }
    }

    async fn finish(&self, id: &str, status: DownloadStatus, error: Option<String>) {
        let snapshot = {
            let mut tasks = self.tasks.write().await;
            match tasks.get_mut(id) {
                Some(task) => {
                    task.status = status;
                    task.error = error;
                    match status {
                        DownloadStatus::Completed => {
                            task.completed_at = Some(now());
                            if task.total.is_none() {
                                task.total = Some(task.downloaded);
                            }
                            if let Some(total) = task.total {
                                task.downloaded = total;
                            }
                            for seg in &mut task.segments {
                                seg.downloaded = seg.len();
                                seg.status = SegmentStatus::Done;
                            }
                        }
                        _ => {
                            task.speed = 0.0;
                            task.eta_secs = None;
                        }
                    }
                    Some(task.clone())
                }
                None => None,
            }
        };

        if let Some(task) = snapshot {
            if status == DownloadStatus::Completed {
                let _ = store::remove(&task);
            } else {
                let _ = store::save(&task);
            }
            let _ = self.events.send(EngineEvent::StateChanged { task });
        }
        self.controls.lock().await.remove(id);
    }
}

/// One byte range being worked on. `end` may shrink when the range is split, and
/// the owning task re-reads it, so a split takes effect at the next chunk.
#[derive(Clone)]
struct Live {
    start: u64,
    end: Arc<AtomicU64>,
    progress: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl Live {
    /// Bytes still to fetch, inclusive.
    fn remaining(&self) -> u64 {
        let pos = self.start + self.progress.load(Ordering::Relaxed);
        let end = self.end.load(Ordering::Relaxed);
        if pos > end {
            0
        } else {
            end - pos + 1
        }
    }
}

fn build_lives(segments: &[Segment]) -> Vec<Live> {
    segments
        .iter()
        .map(|seg| Live {
            start: seg.start,
            end: Arc::new(AtomicU64::new(seg.end)),
            progress: Arc::new(AtomicU64::new(seg.downloaded)),
            running: Arc::new(AtomicBool::new(false)),
        })
        .collect()
}

/// Snapshot the live ranges as persisted `Segment`s, for the task list and UI.
fn lives_to_segments(lives: &[Live]) -> Vec<Segment> {
    lives
        .iter()
        .enumerate()
        .map(|(index, live)| {
            let end = live.end.load(Ordering::Relaxed);
            let done = live.progress.load(Ordering::Relaxed);
            let finished = live.start + done > end;
            Segment {
                index: index as u32,
                start: live.start,
                end,
                downloaded: if finished {
                    end.saturating_sub(live.start) + 1
                } else {
                    done
                },
                status: if finished {
                    SegmentStatus::Done
                } else if live.running.load(Ordering::Relaxed) {
                    SegmentStatus::Active
                } else {
                    SegmentStatus::Pending
                },
            }
        })
        .collect()
}

/// Claim the next unfinished, not-running range.
fn claim_next(lives: &Arc<StdMutex<Vec<Live>>>) -> Option<Live> {
    let mut guard = lives.lock().unwrap();
    let live = guard
        .iter_mut()
        .find(|live| !live.running.load(Ordering::Relaxed) && live.remaining() > 0)?;
    live.running.store(true, Ordering::Relaxed);
    Some(live.clone())
}

/// Split the largest remaining range roughly in half, if worth it. The existing
/// range keeps the downloaded prefix; the tail becomes a new, unclaimed range.
fn split_largest(lives: &Arc<StdMutex<Vec<Live>>>, min_len: u64) -> bool {
    let mut guard = lives.lock().unwrap();
    let idx = guard
        .iter()
        .enumerate()
        .filter(|(_, live)| live.remaining() >= min_len.saturating_mul(2))
        .max_by_key(|(_, live)| live.remaining())
        .map(|(i, _)| i);
    let Some(idx) = idx else {
        return false;
    };

    let (start, done, end) = {
        let live = &guard[idx];
        (
            live.start,
            live.progress.load(Ordering::Relaxed),
            live.end.load(Ordering::Relaxed),
        )
    };
    let pos = start + done;
    if pos > end {
        return false;
    }
    let remaining = end - pos + 1;
    let tail_start = pos + remaining / 2;
    if tail_start <= pos || tail_start > end {
        return false;
    }

    guard[idx].end.store(tail_start - 1, Ordering::Relaxed);
    guard.push(Live {
        start: tail_start,
        end: Arc::new(AtomicU64::new(end)),
        progress: Arc::new(AtomicU64::new(0)),
        running: Arc::new(AtomicBool::new(false)),
    });
    true
}

async fn download_live(
    client: &Client,
    spec: &DownloadSpec,
    out_path: &Path,
    live: &Live,
    global: &Arc<RateLimiter>,
    control: &Control,
) -> Result<()> {
    let opts = RequestOptions::from_spec(spec);
    let mut stalls = 0u32;

    loop {
        if control.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if control.paused.load(Ordering::Relaxed) {
            return Err(Error::Paused);
        }

        // Re-read each pass: a concurrent split may have shrunk our range.
        let end = live.end.load(Ordering::Relaxed);
        let done = live.progress.load(Ordering::Relaxed);
        let start = live.start + done;
        if start > end {
            break;
        }

        let before = done;
        let resp = http::fetch_range(client, &spec.url, start, end, &opts).await?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_path)
            .await?;
        file.seek(SeekFrom::Start(start)).await?;

        let mut written = done;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if control.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if control.paused.load(Ordering::Relaxed) {
                return Err(Error::Paused);
            }
            let chunk = chunk?;
            let len = chunk.len() as u64;
            global.acquire(len).await;
            control.limiter.acquire(len).await;
            file.write_all(&chunk).await?;
            written += len;
            live.progress.store(written, Ordering::Relaxed);
            // A split may have moved our end earlier; stop and let the tail run.
            if live.start + written > live.end.load(Ordering::Relaxed) {
                break;
            }
        }
        file.flush().await?;

        if written == before {
            stalls += 1;
            if stalls > 8 {
                return Err(Error::internal(format!(
                    "segment stalled at byte {}",
                    live.start + written
                )));
            }
            tokio::time::sleep(Duration::from_millis(250 * stalls as u64)).await;
        } else {
            stalls = 0;
        }
    }
    Ok(())
}

/// Copy the first `want` bytes of an already-open response into `path` at
/// `offset`, honouring the limiters and the pause/cancel flags. Returns how many
/// bytes were written.
///
/// Used for the body the opening request already delivered: those bytes would
/// otherwise be thrown away and fetched a second time.
async fn write_prefix(
    response: Response,
    path: &Path,
    offset: u64,
    want: u64,
    global: &Arc<RateLimiter>,
    control: &Control,
) -> Result<u64> {
    let mut file = OpenOptions::new().write(true).open(path).await?;
    file.seek(SeekFrom::Start(offset)).await?;

    let mut stream = response.bytes_stream();
    let mut written = 0u64;
    while written < want {
        if control.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if control.paused.load(Ordering::Relaxed) {
            return Err(Error::Paused);
        }
        let Some(chunk) = stream.next().await else {
            break;
        };
        let chunk = chunk?;
        let take = chunk.len().min((want - written) as usize);
        global.acquire(take as u64).await;
        control.limiter.acquire(take as u64).await;
        file.write_all(&chunk[..take]).await?;
        written += take as u64;
    }
    file.flush().await?;
    Ok(written)
}

async fn preallocate(path: &Path, total: u64) -> Result<()> {
    let file = OpenOptions::new().write(true).create(true).open(path).await?;
    file.set_len(total).await?;
    Ok(())
}

/// Remux an MPEG-TS stream into `.mp4` (stream copy, no re-encode) when ffmpeg
/// is available. Returns the new path, or `None` — leaving the `.ts` — if the
/// copy fails, since a playable TS beats a broken MP4.
async fn remux_to_mp4(path: &Path) -> Option<PathBuf> {
    let ffmpeg = util::find_executable("ffmpeg")?;
    let target = path.with_extension("mp4");
    let status = tokio::process::Command::new(ffmpeg)
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-c", "copy", "-bsf:a", "aac_adtstoasc"])
        .arg(&target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .ok()?;
    if !status.success() || tokio::fs::metadata(&target).await.is_err() {
        let _ = tokio::fs::remove_file(&target).await;
        return None;
    }
    let _ = tokio::fs::remove_file(path).await;
    Some(target)
}

/// The name a download shows until the probe (or `yt-dlp`) knows better.
///
/// A route stem like `watch` says nothing about the file, so what the page calls
/// itself wins there — that is what stops `youtube.com/watch?v=…` from being
/// saved as `watch.mp4`.
fn provisional_name(spec: &DownloadSpec) -> String {
    let mut from_url = util::filename_from_url(&spec.url);
    if spec.kind == DownloadKind::Hls && from_url.to_ascii_lowercase().ends_with(".m3u8") {
        from_url.truncate(from_url.len() - 5);
        from_url.push_str(".ts");
    }

    let Some(title) = spec.title.as_deref() else {
        return from_url;
    };
    if !util::is_generic_stem(&from_url) {
        return from_url;
    }
    let clean = util::sanitize_filename(title);
    if clean.is_empty() {
        return from_url;
    }
    match util::extension_of(&from_url) {
        Some(ext) => format!("{clean}.{ext}"),
        None => clean,
    }
}

/// yt-dlp's last stderr line, plus an actionable hint when we have one.
fn ytdlp_failure(stderr: &str, fallback: &str) -> String {
    let detail = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .filter(|line| line.contains("ERROR") || !line.is_empty())
        .unwrap_or(fallback);
    match ytdlp::hint(stderr) {
        Some(hint) => format!("{detail} — {hint}"),
        None => detail.to_string(),
    }
}

fn not_found(id: &str) -> Error {
    Error::NotFound(id.to_string())
}

/// The file yt-dlp produced: by stem when we named it, otherwise the newest
/// media file in the directory (yt-dlp named it after the video's own title).
async fn produced_file(dir: &Path, stem: Option<&str>) -> Option<String> {
    let prefix = stem.map(|stem| format!("{stem}."));
    let mut newest: Option<(SystemTime, String)> = None;
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(prefix) = &prefix {
            if name.starts_with(prefix) {
                return Some(name);
            }
            continue;
        }
        // No prefix to match on: skip the partials and sidecars yt-dlp leaves.
        if name.ends_with(".part")
            || name.ends_with(".ytdl")
            || name.ends_with(".temp")
            || name.starts_with('.')
        {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if meta.is_dir() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if newest.as_ref().map(|(at, _)| modified > *at).unwrap_or(true) {
            newest = Some((modified, name));
        }
    }
    newest.map(|(_, name)| name)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lives_round_trip_preserves_progress() {
        let mut segments = segment::plan(1000, 2);
        segments[0].downloaded = 100;
        let back = lives_to_segments(&build_lives(&segments));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].start, segments[0].start);
        assert_eq!(back[0].end, segments[0].end);
        assert_eq!(back[0].downloaded, 100);
    }

    #[test]
    fn claim_marks_running_and_skips_the_rest() {
        let lives = Arc::new(StdMutex::new(build_lives(&segment::plan(1000, 2))));
        let first = claim_next(&lives).expect("first claim");
        assert!(first.running.load(Ordering::Relaxed));
        let second = claim_next(&lives).expect("second claim");
        assert_ne!(first.start, second.start);
        assert!(claim_next(&lives).is_none(), "both ranges are taken");
    }

    #[test]
    fn split_conserves_bytes_and_keeps_the_prefix() {
        let mut segments = segment::plan(1000, 1);
        segments[0].downloaded = 200;
        let lives = Arc::new(StdMutex::new(build_lives(&segments)));

        let before: u64 = lives.lock().unwrap().iter().map(|l| l.remaining()).sum();
        assert!(split_largest(&lives, 10));
        let guard = lives.lock().unwrap();
        let after: u64 = guard.iter().map(|l| l.remaining()).sum();
        assert_eq!(before, after, "a split must not lose bytes");
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0].start + guard[0].progress.load(Ordering::Relaxed), 200);
        assert_eq!(guard[0].end.load(Ordering::Relaxed) + 1, guard[1].start);
        assert_eq!(guard[1].end.load(Ordering::Relaxed), 999);
    }

    #[test]
    fn split_declines_when_below_the_floor() {
        let lives = Arc::new(StdMutex::new(build_lives(&segment::plan(100, 2))));
        assert!(!split_largest(&lives, 1000));
        assert_eq!(lives.lock().unwrap().len(), 2);
    }
}
