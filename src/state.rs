//! UI-facing state: the process-wide engine handle, a polled mirror of its
//! task list, and the actions the interface can dispatch.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dioxus::prelude::*;
use rdm_core::{
    Category, DownloadManager, DownloadSpec, DownloadStatus, DownloadTask, EngineConfig,
};

/// The single engine instance, shared by every component.
static ENGINE: OnceLock<Arc<DownloadManager>> = OnceLock::new();

pub fn engine() -> Arc<DownloadManager> {
    ENGINE
        .get_or_init(|| {
            DownloadManager::new(EngineConfig::default())
                .expect("failed to construct the RDM download engine")
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

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All downloads",
            Filter::Active => "Active",
            Filter::Queued => "Queued",
            Filter::Finished => "Finished",
            Filter::Unfinished => "Unfinished",
            Filter::Compressed => "Compressed",
            Filter::Documents => "Documents",
            Filter::Music => "Music",
            Filter::Programs => "Programs",
            Filter::Video => "Video",
            Filter::Images => "Images",
            Filter::Other => "Other",
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
    pub connections: Signal<u8>,
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

    pub fn set_connections(&self, n: u8) {
        set(self.connections, n.clamp(1, 32));
    }

    pub fn add(&self, url: String, dir: PathBuf) {
        let connections = *self.connections.read();
        let spec = DownloadSpec::new(url, dir).connections(connections);
        spawn(async move {
            let _ = engine().add(spec).await;
        });
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

    pub fn selected_ids(&self) -> Vec<String> {
        self.selected.read().clone().into_iter().collect()
    }
}

fn set<T: 'static>(mut signal: Signal<T>, value: T) {
    signal.set(value);
}

pub fn file_name(task: &DownloadTask) -> String {
    task.output_path()
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| task.id.clone())
}

/// Mount the engine mirror. The engine lives on the app's async runtime; we
/// poll it a few times a second and push snapshots into a signal so the UI
/// stays reactive. (Event streaming can replace the poll once the UI settles.)
pub fn use_downloads() -> Downloads {
    let tasks = use_signal(Vec::<DownloadTask>::new);
    let selected = use_signal(|| None::<String>);
    let filter = use_signal(Filter::default);
    let query = use_signal(String::new);
    let show_add = use_signal(|| false);
    let connections = use_signal(|| 8u8);

    use_future(move || async move {
        let mut tasks = tasks;
        loop {
            tasks.set(engine().list().await);
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });

    Downloads {
        tasks,
        selected,
        filter,
        query,
        show_add,
        connections,
    }
}
