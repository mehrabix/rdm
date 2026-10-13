//! UI-facing state: the process-wide engine handle, a polled mirror of its
//! task list, and the actions the interface can dispatch.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dioxus::prelude::*;
use rdm_core::{
    Category, DownloadManager, DownloadSpec, DownloadStatus, DownloadTask, EngineConfig, Found,
};

use crate::settings;

/// The single engine instance, shared by every component. Sized from the
/// settings loaded at startup.
static ENGINE: OnceLock<Arc<DownloadManager>> = OnceLock::new();

pub fn engine() -> Arc<DownloadManager> {
    ENGINE
        .get_or_init(|| {
            let boot = settings::boot();
            let config = EngineConfig {
                max_concurrent: boot.max_concurrent.max(1) as usize,
                default_connections: boot.connections.max(1),
                queue_limits: boot.queue_limits(),
                ytdlp_cookies_from_browser: (!boot.browser_cookies.trim().is_empty())
                    .then(|| boot.browser_cookies.clone()),
                ..EngineConfig::default()
            };
            DownloadManager::new(config).expect("failed to construct the RDM download engine")
        })
        .clone()
}

/// Left-hand category rail, mirroring IDM's buckets.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Active,
    Queued,
    Finished,
    Unfinished,
    Compressed,
    Documents,
    Music,
    Programs,
    Video,
    Images,
    Other,
}

impl Filter {
    pub const ALL: [Filter; 12] = [
        Filter::All,
        Filter::Active,
        Filter::Queued,
        Filter::Finished,
        Filter::Unfinished,
        Filter::Compressed,
        Filter::Documents,
        Filter::Music,
        Filter::Programs,
        Filter::Video,
        Filter::Images,
        Filter::Other,
    ];

    /// i18n key for this filter's label.
    pub fn label_key(self) -> &'static str {
        match self {
            Filter::All => "sidebar.all",
            Filter::Active => "sidebar.active",
            Filter::Queued => "sidebar.queued",
            Filter::Finished => "sidebar.finished",
            Filter::Unfinished => "sidebar.unfinished",
            Filter::Compressed => "sidebar.compressed",
            Filter::Documents => "sidebar.documents",
            Filter::Music => "sidebar.music",
            Filter::Programs => "sidebar.programs",
            Filter::Video => "sidebar.video",
            Filter::Images => "sidebar.images",
            Filter::Other => "sidebar.other",
        }
    }

    pub fn matches(self, task: &DownloadTask) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => task.status.is_active(),
            Filter::Queued => task.status == DownloadStatus::Queued,
            Filter::Finished => task.status == DownloadStatus::Completed,
            Filter::Unfinished => !task.status.is_terminal(),
            Filter::Compressed => task.category == Category::Compressed,
            Filter::Documents => task.category == Category::Documents,
            Filter::Music => task.category == Category::Music,
            Filter::Programs => task.category == Category::Programs,
            Filter::Video => task.category == Category::Video,
            Filter::Images => task.category == Category::Images,
            Filter::Other => task.category == Category::Other,
        }
    }
}

/// All the reactive handles a view needs. Cheap to copy and pass as props.
#[derive(Clone, Copy, PartialEq)]
pub struct Downloads {
    pub tasks: Signal<Vec<DownloadTask>>,
    pub selected: Signal<Option<String>>,
    pub filter: Signal<Filter>,
    pub query: Signal<String>,
    pub show_add: Signal<bool>,
    pub show_batch: Signal<bool>,
    pub show_options: Signal<bool>,
    pub connections: Signal<u8>,
    /// Queue new downloads are assigned to.
    pub queue: Signal<String>,
    /// Site grabber: the dialog, its results, and the ticked URLs.
    pub show_grab: Signal<bool>,
    pub grab_results: Signal<Vec<Found>>,
    pub grab_selected: Signal<Vec<String>>,
    pub grab_busy: Signal<bool>,
    pub grab_error: Signal<Option<String>>,
}

impl Downloads {
    pub fn visible(&self) -> Vec<DownloadTask> {
        let query = self.query.read().trim().to_lowercase();
        let filter = *self.filter.read();
        self.tasks
            .read()
            .iter()
            .filter(|task| filter.matches(task))
            .filter(|task| query.is_empty() || file_name(task).to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    pub fn count(&self, filter: Filter) -> usize {
        self.tasks.read().iter().filter(|t| filter.matches(t)).count()
    }

    pub fn total_speed(&self) -> f64 {
        self.tasks.read().iter().map(|t| t.speed).sum()
    }

    pub fn active_count(&self) -> usize {
        self.tasks
            .read()
            .iter()
            .filter(|t| t.status.is_active())
            .count()
    }

    pub fn queued_count(&self) -> usize {
        self.tasks
            .read()
            .iter()
            .filter(|t| t.status == DownloadStatus::Queued)
            .count()
    }

    // ---- view toggles ----------------------------------------------------

    pub fn set_filter(&self, filter: Filter) {
        set(self.filter, filter);
    }

    pub fn set_query(&self, value: String) {
        set(self.query, value);
    }

    pub fn select(&self, id: Option<String>) {
        set(self.selected, id);
    }

    pub fn open_add(&self) {
        set(self.show_add, true);
    }

    pub fn close_add(&self) {
        set(self.show_add, false);
    }

    // ---- site grabber ----------------------------------------------------

    pub fn open_grab(&self) {
        set(self.show_grab, true);
    }

    pub fn close_grab(&self) {
        set(self.show_grab, false);
    }

    /// Tick or untick one grabbed URL.
    pub fn toggle_grab(&self, url: String) {
        let mut selected = self.grab_selected;
        let mut current = selected.read().clone();
        match current.iter().position(|item| *item == url) {
            Some(index) => {
                current.remove(index);
            }
            None => current.push(url),
        }
        selected.set(current);
    }

    /// Tick everything, or nothing.
    pub fn select_all_grab(&self, all: bool) {
        let urls: Vec<String> = self
            .grab_results
            .read()
            .iter()
            .map(|item| item.url.clone())
            .collect();
        set(self.grab_selected, if all { urls } else { Vec::new() });
    }

    pub fn open_options(&self) {
        set(self.show_options, true);
    }

    pub fn close_options(&self) {
        set(self.show_options, false);
    }

    pub fn set_connections(&self, n: u8) {
        set(self.connections, n.clamp(1, 32));
    }

    /// Stop a queue: its running transfers pause and new ones wait.
    pub fn stop_queue(&self, queue: String) {
        spawn(async move {
            engine().stop_queue(&queue).await;
        });
    }

    /// Start a queue again, resuming what it paused.
    pub fn start_queue(&self, queue: String) {
        let engine = engine();
        spawn(async move {
            engine.start_queue(&queue).await;
        });
    }

    // ---- engine actions --------------------------------------------------

    /// Fire-and-forget queue (menu / clipboard / batch paths). Errors are logged.
    pub fn add(&self, url: String, dir: PathBuf, organize: bool) {
        let state = *self;
        spawn(async move {
            if let Err(err) = state.try_add(url, dir, organize).await {
                tracing::error!(%err, "could not queue download");
            }
        });
    }

    /// Set one download's own speed cap (KB/s; 0 = unlimited).
    pub fn set_speed_limit(&self, id: String, kbps: u32) {
        spawn(async move {
            if let Err(err) = engine()
                .set_download_speed_limit(&id, kbps as u64 * 1024)
                .await
            {
                tracing::error!(%err, "could not set the download speed limit");
            }
        });
    }

    /// Queue a URL and report failures to the caller. An identical URL that is
    /// still queued/running/finished is not queued twice — the existing row is
    /// selected instead. Returns the engine error so the UI can show it.
    pub async fn try_add(
        &self,
        url: String,
        dir: PathBuf,
        organize: bool,
    ) -> rdm_core::Result<()> {
        if let Some(existing) = self.find_by_url(&url) {
            self.select(Some(existing));
            return Ok(());
        }

        let connections = *self.connections.read();
        let queue = self.queue.read().clone();
        let mut spec = DownloadSpec::new(url, dir).connections(connections);
        spec.queue = queue;
        // The engine picks the per-category folder after it probes.
        spec.organize = organize;

        engine().add(spec).await.map(|_| ())
    }

    /// Id of an existing task for `url`, whatever its state. Used to avoid
    /// queuing the same URL twice (the clipboard monitor can see a URL more than
    /// once as the clipboard changes).
    fn find_by_url(&self, url: &str) -> Option<String> {
        self.tasks
            .read()
            .iter()
            .find(|task| task.spec.url == url)
            .map(|task| task.id.clone())
    }

    /// Queue every http(s) URL in `raw` — newline separated, or a path to a
    /// `.txt` file. Returns how many were queued.
    pub fn import_text(&self, raw: &str, base: PathBuf, organize: bool) -> usize {
        let trimmed = raw.trim();
        let content = if !trimmed.contains('\n') && trimmed.ends_with(".txt") {
            std::fs::read_to_string(trimmed).unwrap_or_else(|_| raw.to_string())
        } else {
            raw.to_string()
        };
        let mut queued = 0;
        for line in content.lines() {
            let url = line.trim();
            if url.starts_with("http://") || url.starts_with("https://") {
                self.add(url.to_string(), base.clone(), organize);
                queued += 1;
            }
        }
        queued
    }

    pub fn pause(&self, id: String) {
        spawn(async move {
            let _ = engine().pause(&id).await;
        });
    }

    pub fn resume(&self, id: String) {
        spawn(async move {
            let _ = engine().resume(&id).await;
        });
    }

    pub fn cancel(&self, id: String) {
        spawn(async move {
            let _ = engine().cancel(&id).await;
        });
    }

    pub fn remove(&self, id: String) {
        spawn(async move {
            let _ = engine().remove(&id, true).await;
        });
    }

    /// Ids of every transfer that is running or waiting.
    pub fn running_ids(&self) -> Vec<String> {
        self.tasks
            .read()
            .iter()
            .filter(|t| t.status.is_active() || t.status == DownloadStatus::Queued)
            .map(|t| t.id.clone())
            .collect()
    }

    /// Pause every transfer that is running or waiting.
    pub fn pause_all(&self) {
        for id in self.running_ids() {
            spawn(async move {
                let _ = engine().pause(&id).await;
            });
        }
    }

    /// Resume every transfer that is stopped but recoverable.
    pub fn resume_all(&self) {
        let ids: Vec<String> = self
            .tasks
            .read()
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    DownloadStatus::Paused | DownloadStatus::Failed | DownloadStatus::Cancelled
                )
            })
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            spawn(async move {
                let _ = engine().resume(&id).await;
            });
        }
    }

    /// Forget every transfer that has finished.
    pub fn remove_completed(&self) {
        let ids: Vec<String> = self
            .tasks
            .read()
            .iter()
            .filter(|t| t.status == DownloadStatus::Completed)
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            spawn(async move {
                let _ = engine().remove(&id, false).await;
            });
        }
    }

    /// Queue every http(s) URL found on the system clipboard; if there is
    /// nothing usable, open the Add dialog so the user can paste manually.
    pub fn import_clipboard(&self) {
        let boot = settings::boot();
        let dir = boot.download_dir.clone();
        let organize = boot.organize_by_category;
        if self.import_text(&clipboard_text(), dir, organize) == 0 {
            self.open_add();
        }
    }

    /// Reveal a completed download in the OS file manager.
    pub fn open_folder(&self, path: PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }

    /// Open a completed file with the desktop's default application.
    pub fn open_file(&self, path: PathBuf) {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }

    pub fn selected_ids(&self) -> Vec<String> {
        self.selected.read().clone().into_iter().collect()
    }
}

fn set<T: 'static>(mut signal: Signal<T>, value: T) {
    signal.set(value);
}

/// Read text from the system clipboard (Wayland first, then X11), with a hard
/// timeout so a stuck helper can never wedge the app. Empty string when neither
/// tool is available.
pub fn clipboard_text() -> String {
    let readers: [(&str, &[&str]); 2] = [
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
    ];
    for (cmd, args) in readers {
        if let Some(text) = run_capture(cmd, args, std::time::Duration::from_millis(1500)) {
            return text;
        }
    }
    String::new()
}

/// Run a command capturing stdout, killing it if it outlives `timeout`.
fn run_capture(cmd: &str, args: &[&str], timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut text = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut text);
                }
                return Some(text);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

pub fn file_name(task: &DownloadTask) -> String {
    task.output_path()
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| task.id.clone())
}

/// Mount the engine mirror. On first run we restore the persisted queue, then
/// poll the engine a few times a second and periodically snapshot the queue to
/// disk so a restart picks up where we left off.
pub fn use_downloads() -> Downloads {
    let tasks = use_signal(Vec::<DownloadTask>::new);
    let selected = use_signal(|| None::<String>);
    let filter = use_signal(Filter::default);
    let query = use_signal(String::new);
    let show_add = use_signal(|| false);
    let show_batch = use_signal(|| false);
    let show_options = use_signal(|| false);
    let show_grab = use_signal(|| false);
    let grab_results = use_signal(Vec::<Found>::new);
    let grab_selected = use_signal(Vec::<String>::new);
    let grab_busy = use_signal(|| false);
    let grab_error = use_signal(|| None::<String>);
    let connections = use_signal(|| settings::boot().connections.max(1));
    let queue = use_signal(|| {
        settings::boot()
            .queue_names()
            .first()
            .cloned()
            .unwrap_or_else(|| "Main".to_string())
    });

    use_future(move || async move {
        let mut tasks = tasks;

        // Restore anything left over from a previous session.
        let queue_path = rdm_core::store::default_queue_path();
        for task in rdm_core::store::load_queue(&queue_path) {
            let _ = engine().restore(task).await;
        }

        let mut ticks: u32 = 0;
        loop {
            let snapshot = engine().list().await;
            tasks.set(snapshot.clone());

            // Persist roughly every 3 seconds (10 x 300 ms).
            ticks = ticks.wrapping_add(1);
            if ticks % 10 == 0 {
                let path = queue_path.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = rdm_core::store::save_queue(&snapshot, &path);
                })
                .await;
            }

            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });

    Downloads {
        tasks,
        selected,
        filter,
        query,
        show_add,
        show_batch,
        show_options,
        show_grab,
        grab_results,
        grab_selected,
        grab_busy,
        grab_error,
        connections,
        queue,
    }
}
