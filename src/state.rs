//! UI-facing state: the process-wide engine handle, a polled mirror of its
//! task list, and the actions the interface can dispatch.

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use dioxus::prelude::*;
use rdm_core::{
    Category, DownloadManager, DownloadSpec, DownloadStatus, DownloadTask, EngineConfig, Found,
};

use crate::settings::{self, OfferSource, Settings};

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

/// The browser gate: [`rdm_core::ipc::serve`] holds the sender, and the app
/// drains the receiver so each handed-over download can be confirmed first.
static GATE: OnceLock<tokio::sync::mpsc::UnboundedSender<DownloadSpec>> = OnceLock::new();

/// Create the gate channel once, returning the receiving end for the app.
pub fn init_gate() -> tokio::sync::mpsc::UnboundedReceiver<DownloadSpec> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let _ = GATE.set(sender);
    receiver
}

/// The sender the IPC server needs, available once the app installed the gate.
pub fn gate_sender() -> Option<tokio::sync::mpsc::UnboundedSender<DownloadSpec>> {
    GATE.get().cloned()
}

/// The last text this app put on the clipboard itself, so that "Copy link"
/// does not come straight back through the clipboard monitor as a "detected"
/// download — the most annoying possible loop.
static SELF_COPIED: OnceLock<StdMutex<Option<String>>> = OnceLock::new();

fn self_copied_slot() -> &'static StdMutex<Option<String>> {
    SELF_COPIED.get_or_init(|| StdMutex::new(None))
}

/// Copy `text` to the system clipboard, and remember it as ours.
///
/// Best effort, and X11 first: Wayland's clipboard tools make the compositor
/// raise a transient popup surface on some setups, while XWayland's `xclip`
/// serves the same selection without that side effect. A failed write is
/// harmless — the value is still recorded so the monitor stays quiet.
pub fn copy_to_clipboard(text: &str) {
    if let Ok(mut slot) = self_copied_slot().lock() {
        *slot = Some(text.to_string());
    }

    let x11: [(&str, &[&str]); 1] = [("xclip", &["-selection", "clipboard"])];
    let wayland: [(&str, &[&str]); 1] = [("wl-copy", &[])];
    let writers: Vec<(&str, &[&str])> = if std::env::var_os("DISPLAY").is_some() {
        x11.iter().chain(wayland.iter()).copied().collect()
    } else {
        wayland.iter().chain(x11.iter()).copied().collect()
    };

    for (cmd, args) in writers {
        let spawned = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn();
        if let Ok(mut child) = spawned {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
}

/// Whether `text` is what this app last copied itself.
pub fn self_copied(text: &str) -> bool {
    self_copied_slot()
        .lock()
        .map(|slot| slot.as_deref() == Some(text))
        .unwrap_or(false)
}

/// URLs cancelled in this session. Asking again about a link the user already
/// declined is the other annoying half of clipboard capture.
static DECLINED: OnceLock<StdMutex<Vec<String>>> = OnceLock::new();
const DECLINED_LIMIT: usize = 200;

fn declined_slot() -> &'static StdMutex<Vec<String>> {
    DECLINED.get_or_init(|| StdMutex::new(Vec::new()))
}

/// Remember that the user said no to this URL.
pub fn decline(url: &str) {
    if let Ok(mut declined) = declined_slot().lock() {
        if declined.len() >= DECLINED_LIMIT {
            declined.remove(0);
        }
        if !declined.iter().any(|entry| entry == url) {
            declined.push(url.to_string());
        }
    }
}

/// Whether the user already declined this URL in this session.
pub fn is_declined(url: &str) -> bool {
    declined_slot()
        .lock()
        .map(|declined| declined.iter().any(|entry| entry == url))
        .unwrap_or(false)
}

/// Whether an existing task should absorb another request for the same URL.
/// Finished downloads do (the user sees the row they already have); failures do
/// not, because the user asking again means "try it again".
fn blocks_repeat(status: DownloadStatus) -> bool {
    !matches!(
        status,
        DownloadStatus::Failed | DownloadStatus::Cancelled
    )
}

/// Whether the confirmation window is already showing this download.
fn same_offer(open: &PendingDownload, pending: &PendingDownload) -> bool {
    open.url == pending.url
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::OfferSource;

    fn pending(url: &str, source: OfferSource) -> PendingDownload {
        PendingDownload {
            url: url.to_string(),
            dir: PathBuf::from("/tmp"),
            file_name: None,
            queue: "Main".to_string(),
            connections: 8,
            referer: None,
            cookies: None,
            user_agent: None,
            content_type: None,
            height: None,
            title: None,
            keep_name: false,
            source,
        }
    }

    /// A failed download must not stop the user from trying the same URL again.
    #[test]
    fn only_live_or_finished_downloads_block_a_repeat() {
        assert!(blocks_repeat(DownloadStatus::Queued));
        assert!(blocks_repeat(DownloadStatus::Downloading));
        assert!(blocks_repeat(DownloadStatus::Paused));
        assert!(blocks_repeat(DownloadStatus::Completed));
        assert!(!blocks_repeat(DownloadStatus::Failed));
        assert!(!blocks_repeat(DownloadStatus::Cancelled));
    }

    /// The same link reported twice must not re-open (and reset) the window.
    #[test]
    fn the_same_offer_is_recognised_whatever_the_source() {
        let open = pending("https://example.test/a.mp4", OfferSource::Clipboard);
        assert!(same_offer(&open, &pending("https://example.test/a.mp4", OfferSource::Browser)));
        assert!(!same_offer(&open, &pending("https://example.test/b.mp4", OfferSource::Clipboard)));
    }

    /// Clipboard capture must not fight itself: RDM's own "Copy link" is
    /// remembered, and a link the user cancelled is not offered again.
    #[test]
    fn our_own_copy_and_declined_links_are_remembered() {
        assert!(!self_copied("https://example.test/a"));
        copy_to_clipboard("https://example.test/a");
        assert!(self_copied("https://example.test/a"));
        assert!(!self_copied("https://example.test/b"), "only the last write counts");

        assert!(!is_declined("https://example.test/declined"));
        decline("https://example.test/declined");
        decline("https://example.test/declined");
        assert!(is_declined("https://example.test/declined"));
        assert!(!is_declined("https://example.test/other"));
    }

    /// The declined list is bounded, so a long session cannot grow it forever.
    #[test]
    fn the_declined_list_is_capped() {
        for index in 0..(DECLINED_LIMIT + 10) {
            decline(&format!("https://example.test/{index}"));
        }
        let len = declined_slot().lock().map(|list| list.len()).unwrap_or(0);
        assert!(len <= DECLINED_LIMIT, "declined list grew to {len}");
        // The oldest entries were dropped, the newest kept.
        assert!(is_declined(&format!("https://example.test/{}", DECLINED_LIMIT + 9)));
    }
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
    /// A download waiting for the user in the "Download File Info" window.
    pub pending: Signal<Option<PendingDownload>>,
}

/// A recognized download that has not started yet — everything the
/// "Download File Info" window shows and edits.
#[derive(Clone, PartialEq)]
pub struct PendingDownload {
    pub url: String,
    pub dir: PathBuf,
    pub file_name: Option<String>,
    pub queue: String,
    pub connections: u8,
    pub referer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub content_type: Option<String>,
    pub height: Option<u32>,
    /// What the page says the file is called (from the browser).
    pub title: Option<String>,
    /// The user edited the file name in the window, so keep it.
    pub keep_name: bool,
    pub source: OfferSource,
}

impl PendingDownload {
    /// A download for `url` filled from the current preferences (the manual and
    /// clipboard cases).
    pub fn new(url: String, settings: &Settings, source: OfferSource) -> Self {
        Self {
            url,
            dir: settings.download_dir.clone(),
            file_name: None,
            queue: settings
                .queue_names()
                .into_iter()
                .next()
                .unwrap_or_else(|| "Main".to_string()),
            connections: settings.connections.max(1),
            referer: None,
            cookies: None,
            user_agent: None,
            content_type: None,
            height: None,
            title: None,
            keep_name: false,
            source,
        }
    }

    /// The same window, for a download the browser handed over: the spec's
    /// cookies, referer and media type must survive the round trip.
    pub fn from_spec(spec: &rdm_core::DownloadSpec, source: OfferSource) -> Self {
        Self {
            url: spec.url.clone(),
            dir: spec.output_dir.clone(),
            file_name: spec.file_name.clone(),
            queue: spec.queue.clone(),
            connections: spec.connections.max(1),
            referer: spec.referer.clone(),
            cookies: spec.cookies.clone(),
            user_agent: spec.user_agent.clone(),
            content_type: spec.content_type.clone(),
            height: spec.max_height,
            title: spec.title.clone(),
            keep_name: spec.keep_name,
            source,
        }
    }

    /// The name the window starts with: the URL's own name, unless that is a
    /// route stem (`watch`) and the page told us what the file actually is.
    pub fn suggested_name(&self) -> String {
        let from_url = rdm_core::util::filename_from_url(&self.url);
        let explicit = self.file_name.clone().unwrap_or_default();
        if !explicit.trim().is_empty() {
            return explicit;
        }
        match &self.title {
            Some(title) if rdm_core::util::is_generic_stem(&from_url) => {
                let clean = rdm_core::util::sanitize_filename(title);
                if clean.is_empty() {
                    from_url
                } else {
                    match rdm_core::util::extension_of(&from_url) {
                        Some(ext) => format!("{clean}.{ext}"),
                        None => clean,
                    }
                }
            }
            _ => from_url,
        }
    }

    /// The spec to queue when the user presses Start (or Download Later).
    pub fn spec(&self, organize: bool, later: bool) -> rdm_core::DownloadSpec {
        let mut spec =
            rdm_core::DownloadSpec::new(self.url.clone(), self.dir.clone())
                .connections(self.connections.max(1));
        spec.queue = self.queue.clone();
        spec.organize = organize;
        spec.file_name = self
            .file_name
            .clone()
            .filter(|name| !name.trim().is_empty());
        spec.referer = self.referer.clone();
        spec.cookies = self.cookies.clone();
        spec.user_agent = self.user_agent.clone();
        spec.content_type = self.content_type.clone();
        spec.max_height = self.height;
        spec.title = self.title.clone();
        spec.keep_name = self.keep_name;
        spec.start_paused = later;
        spec
    }
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

    pub fn open_options(&self) {        set(self.show_options, true);
    }

    /// Close the batch-import window (used when its own task finishes).
    pub fn close_batch(&self) {
        set(self.show_batch, false);
    }

    pub fn close_options(&self) {
        set(self.show_options, false);
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
        let connections = *self.connections.read();
        let queue = self.queue.read().clone();
        let mut spec = DownloadSpec::new(url, dir).connections(connections);
        spec.queue = queue;
        // The engine picks the per-category folder after it probes.
        spec.organize = organize;
        self.try_add_spec(spec).await
    }

    /// The full-spec variant of [`try_add`](Self::try_add): the
    /// "Download File Info" window and the browser gate carry cookies, referer,
    /// media type and queue, which the three-argument form cannot express.
    pub async fn try_add_spec(&self, spec: DownloadSpec) -> rdm_core::Result<()> {
        if let Some(existing) = self.find_by_url(&spec.url) {
            self.select(Some(existing));
            return Ok(());
        }
        engine().add(spec).await.map(|_| ())
    }

    // ---- the "Download File Info" window ---------------------------------

    /// Offer a recognized download from a long-lived scope (the clipboard loop,
    /// the browser gate). The decision happens in [`offer_now`](Self::offer_now).
    pub fn offer(&self, pending: PendingDownload, settings: &Settings) {
        let state = *self;
        let settings = settings.clone();
        spawn(async move {
            state.offer_now(pending, &settings).await;
        });
    }

    /// The awaited form of [`offer`](Self::offer).
    ///
    /// Dialogs must use this and then close themselves: `spawn` ties a task to
    /// the component that created it, so a dialog that closes while the queue
    /// call is still pending would cancel it — which is exactly how "Start
    /// Download" managed to do nothing.
    pub async fn offer_now(&self, pending: PendingDownload, settings: &Settings) {
        // A previous attempt that failed or was cancelled must not block a retry.
        self.clear_dead_attempts(&pending.url);
        // Asking twice about the same URL is noise; show the row it already has.
        if let Some(existing) = self.find_by_url(&pending.url) {
            self.select(Some(existing));
            return;
        }
        // The window is already asking about exactly this download. Re-setting
        // `pending` would reset the fields the user is filling in, so a source
        // that reports the same link twice (a flapping clipboard read) cannot
        // make the window loop.
        let current = self.pending.read().clone();
        if current.as_ref().is_some_and(|open| same_offer(open, &pending)) {
            tracing::debug!(url = %pending.url, "already asking about this download");
            return;
        }
        // The user already said no to this link — but only the clipboard is
        // ignored for it: clicking the badge or typing the URL again is an
        // explicit request, and must not be silently swallowed.
        if pending.source == OfferSource::Clipboard && is_declined(&pending.url) {
            tracing::debug!(url = %pending.url, "already declined; not offering again");
            return;
        }
        if settings::should_confirm(settings, pending.source) {
            set(self.pending, Some(pending));
            return;
        }
        let organize = settings.organize_by_category;
        if let Err(err) = self.try_add_spec(pending.spec(organize, false)).await {
            tracing::error!(%err, "could not queue download");
        }
    }

    /// Queue the pending download and close the window. `later` is IDM's
    /// "Download Later": the row appears paused until it is resumed.
    ///
    /// The download is queued *before* the window closes — clearing `pending`
    /// first would unmount the component that owns this future and cancel the
    /// queue call.
    pub async fn start_pending(
        &self,
        pending: PendingDownload,
        organize: bool,
        later: bool,
    ) -> rdm_core::Result<()> {
        let result = self.try_add_spec(pending.spec(organize, later)).await;
        if result.is_ok() {
            set(self.pending, None);
        }
        result
    }

    /// Close the window without queueing anything — and don't ask about this
    /// URL again for the rest of the session.
    pub fn dismiss_pending(&self) {
        if let Some(pending) = self.pending.read().clone() {
            decline(&pending.url);
        }
        set(self.pending, None);
    }

    /// Id of an existing task for `url` that should absorb a new request: one
    /// still queued/running, or already finished. A **failed or cancelled**
    /// attempt is deliberately ignored — treating it as "already queued" is why
    /// retrying a URL that failed once appeared to do nothing at all.
    fn find_by_url(&self, url: &str) -> Option<String> {
        self.tasks
            .read()
            .iter()
            .find(|task| task.spec.url == url && blocks_repeat(task.status))
            .map(|task| task.id.clone())
    }

    /// Forget the failed/cancelled attempts for `url`, so the retry the user just
    /// asked for is the only row.
    fn clear_dead_attempts(&self, url: &str) {
        let dead: Vec<String> = self
            .tasks
            .read()
            .iter()
            .filter(|task| task.spec.url == url && !blocks_repeat(task.status))
            .map(|task| task.id.clone())
            .collect();
        for id in dead {
            self.remove(id);
        }
    }

    /// Every http(s) URL in `raw` — newline separated, or the path to a `.txt`
    /// file holding them. Shared by the batch window and the clipboard import.
    pub fn urls_in(raw: &str) -> Vec<String> {
        let trimmed = raw.trim();
        let content = if !trimmed.contains('\n') && trimmed.ends_with(".txt") {
            std::fs::read_to_string(trimmed).unwrap_or_else(|_| raw.to_string())
        } else {
            raw.to_string()
        };
        content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("http://") || line.starts_with("https://"))
            .map(str::to_string)
            .collect()
    }

    /// Queue every http(s) URL in `raw` — newline separated, or a path to a
    /// `.txt` file. Returns how many were queued.
    pub fn import_text(&self, raw: &str, base: PathBuf, organize: bool) -> usize {
        let urls = Self::urls_in(raw);
        for url in &urls {
            self.add(url.clone(), base.clone(), organize);
        }
        urls.len()
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

/// Read text from the system clipboard, with a hard timeout so a stuck helper
/// can never wedge the app. Empty string when no tool is available.
///
/// X11 first whenever an X display exists: under Wayland `wl-paste` makes the
/// compositor raise a transient popup surface on every call in some setups,
/// while XWayland's `xclip` reads the same selection without that side effect.
pub fn clipboard_text() -> String {
    let x11: [(&str, &[&str]); 1] = [("xclip", &["-selection", "clipboard", "-o"])];
    let wayland: [(&str, &[&str]); 1] = [("wl-paste", &["--no-newline"])];

    let readers: Vec<(&str, &[&str])> = if std::env::var_os("DISPLAY").is_some() {
        x11.iter().chain(wayland.iter()).copied().collect()
    } else {
        wayland.iter().chain(x11.iter()).copied().collect()
    };

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
    let pending = use_signal(|| None::<PendingDownload>);
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
        pending,
        connections,
        queue,
    }
}
