//! The async orchestrator: turns a [`DownloadSpec`] into a running, observable
//! transfer and exposes pause/resume/cancel controls.
//!
//! Concurrency is bounded by a [`Semaphore`] so the engine never opens more
//! parallel *downloads* than configured, while each download fans out into up
//! to `connections` parallel *segments*.

use std::collections::{BTreeMap, HashMap};
use std::io::SeekFrom;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::stream::{self, StreamExt};
use futures::TryStreamExt;
use reqwest::Client;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{broadcast, Semaphore};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::error::{Error, Result};
use crate::http::{self, RequestOptions};
use crate::segment;
use crate::speed::{eta_secs, SpeedMeter};
use crate::store;
use crate::types::{
    Category, DownloadSpec, DownloadStatus, DownloadTask, ProgressUpdate, RemoteInfo, Segment,
    SegmentStatus,
};
use crate::util;

/// How often the engine emits progress snapshots and flushes to disk.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Tunables for the manager.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum number of transfers running at once; the rest queue up.
    pub max_concurrent: usize,
    /// Default connection count for specs that do not specify one.
    pub default_connections: u8,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            default_connections: 8,
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
}

impl Control {
    fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
            paused: Arc::new(AtomicBool::new(false)),
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
}

impl DownloadManager {
    pub fn new(config: EngineConfig) -> Result<Arc<Self>> {
        let (events, _) = broadcast::channel(2048);
        Ok(Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent.max(1))),
            client: http::build_client()?,
            config,
            tasks: Arc::new(RwLock::new(BTreeMap::new())),
            controls: Arc::new(Mutex::new(HashMap::new())),
            events,
        }))
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
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

    /// Probe a URL, then queue and start a transfer. Returns the new id.
    pub async fn add(self: &Arc<Self>, spec: DownloadSpec) -> Result<String> {
        let opts = RequestOptions::from_spec(&spec);
        let remote = http::probe(&self.client, &spec.url, &opts).await?;

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

        let mut spec = spec;
        let dir = spec.output_dir.clone();
        tokio::fs::create_dir_all(&dir).await?;
        let base_name = spec
            .file_name
            .clone()
            .unwrap_or_else(|| util::sanitize_filename(&remote.file_name));
        let unique = util::unique_path(&dir, &base_name);
        spec.file_name = unique
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string);

        let id = uuid::Uuid::new_v4().to_string();
        let category = Category::from_filename(&base_name);
        let task = DownloadTask {
            id: id.clone(),
            spec,
            status: DownloadStatus::Queued,
            total: remote.total_size,
            remote: Some(remote),
            segments,
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
            .insert(id.clone(), Control::new());
        let _ = self.events.send(EngineEvent::Added { task });
        self.spawn(id.clone());
        Ok(id)
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
        self.controls
            .lock()
            .await
            .insert(id.to_string(), Control::new());
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
        // Wait for a concurrency slot (this is the queue).
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

        self.set_status(&id, DownloadStatus::Downloading).await;
        let outcome = self.transfer(&id).await;
        drop(permit);

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
        let (spec, segments, total, out_path) = {
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

        let counters: Arc<Vec<AtomicU64>> = Arc::new(
            segments
                .iter()
                .map(|s| AtomicU64::new(s.downloaded))
                .collect(),
        );
        let single = Arc::new(AtomicU64::new(0));

        let ticker = self.spawn_ticker(id.to_string(), counters.clone(), single.clone(), total);

        let result = if segments.is_empty() {
            self.stream_single(&spec, &out_path, &opts, &single, &control)
                .await
        } else {
            self.stream_segments(&spec, &segments, &out_path, &counters, total, &control)
                .await
        };

        ticker.abort();
        self.sync_progress(id, &counters).await;
        result
    }

    async fn stream_segments(
        self: &Arc<Self>,
        spec: &DownloadSpec,
        segments: &[Segment],
        out_path: &Path,
        counters: &Arc<Vec<AtomicU64>>,
        total: Option<u64>,
        control: &Control,
    ) -> Result<()> {
        let connections = spec.connections.max(1) as usize;
        let client = self.client.clone();

        let jobs = segments.iter().cloned().enumerate().map(|(i, seg)| {
            let client = client.clone();
            let spec = spec.clone();
            let out_path = out_path.to_path_buf();
            let counters = counters.clone();
            let control = control.clone();
            async move {
                if seg.is_done() {
                    return Ok(());
                }
                download_segment(&client, &spec, &out_path, &seg, i, &counters, total, &control).await
            }
        });

        stream::iter(jobs)
            .buffer_unordered(connections)
            .try_collect::<Vec<()>>()
            .await?;
        Ok(())
    }

    async fn stream_single(
        &self,
        spec: &DownloadSpec,
        out_path: &Path,
        opts: &RequestOptions,
        single: &Arc<AtomicU64>,
        control: &Control,
    ) -> Result<()> {
        let resp = http::fetch_full(&self.client, &spec.url, opts).await?;
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
            file.write_all(&chunk).await?;
            written += chunk.len() as u64;
            single.store(written, Ordering::Relaxed);
        }
        file.flush().await?;
        Ok(())
    }

    fn spawn_ticker(
        &self,
        id: String,
        counters: Arc<Vec<AtomicU64>>,
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
                let downloaded = current_bytes(&counters, &single);
                let elapsed = last_t.elapsed();
                meter.observe(downloaded.saturating_sub(last), elapsed);
                last = downloaded;
                last_t = Instant::now();
                let speed = meter.speed();
                let eta = total
                    .and_then(|t| eta_secs(t.saturating_sub(downloaded), speed));

                let update = {
                    let mut tasks = tasks.write().await;
                    match tasks.get_mut(&id) {
                        Some(task) if !task.status.is_terminal() => {
                            task.downloaded = downloaded;
                            task.speed = speed;
                            task.eta_secs = eta;
                            for (i, counter) in counters.iter().enumerate() {
                                if let Some(seg) = task.segments.get_mut(i) {
                                    seg.downloaded = counter.load(Ordering::Relaxed);
                                    seg.status = if seg.is_done() {
                                        SegmentStatus::Done
                                    } else if seg.status != SegmentStatus::Done {
                                        SegmentStatus::Active
                                    } else {
                                        seg.status
                                    };
                                }
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

    async fn sync_progress(&self, id: &str, counters: &Arc<Vec<AtomicU64>>) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            for (i, counter) in counters.iter().enumerate() {
                if let Some(seg) = task.segments.get_mut(i) {
                    seg.downloaded = counter.load(Ordering::Relaxed);
                }
            }
            task.downloaded = task.segments.iter().map(|s| s.downloaded).sum();
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

async fn download_segment(
    client: &Client,
    spec: &DownloadSpec,
    out_path: &Path,
    seg: &Segment,
    index: usize,
    counters: &Arc<Vec<AtomicU64>>,
    _total: Option<u64>,
    control: &Control,
) -> Result<()> {
    let opts = RequestOptions::from_spec(spec);
    let mut downloaded = seg.downloaded;
    let mut stalls = 0u32;

    while downloaded < seg.len() {
        if control.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if control.paused.load(Ordering::Relaxed) {
            return Err(Error::Paused);
        }

        let before = downloaded;
        let start = seg.start + downloaded;
        let resp = http::fetch_range(client, &spec.url, start, seg.end, &opts).await?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_path)
            .await?;
        file.seek(SeekFrom::Start(start)).await?;

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if control.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if control.paused.load(Ordering::Relaxed) {
                return Err(Error::Paused);
            }
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            counters[index].store(downloaded, Ordering::Relaxed);
        }
        file.flush().await?;

        if downloaded == before {
            stalls += 1;
            if stalls > 8 {
                return Err(Error::internal(format!(
                    "segment {index} stalled at byte {downloaded}"
                )));
            }
            tokio::time::sleep(Duration::from_millis(250 * stalls as u64)).await;
        } else {
            stalls = 0;
        }
    }
    Ok(())
}

fn current_bytes(counters: &Arc<Vec<AtomicU64>>, single: &Arc<AtomicU64>) -> u64 {
    if counters.is_empty() {
        single.load(Ordering::Relaxed)
    } else {
        counters.iter().map(|c| c.load(Ordering::Relaxed)).sum()
    }
}

async fn preallocate(path: &Path, total: u64) -> Result<()> {
    let file = OpenOptions::new().write(true).create(true).open(path).await?;
    file.set_len(total).await?;
    Ok(())
}

fn not_found(id: &str) -> Error {
    Error::NotFound(id.to_string())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
