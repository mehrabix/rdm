//! On-disk persistence of in-progress transfers.
//!
//! Each partial download gets a sidecar next to the target file
//! (`movie.mp4` -> `movie.mp4.rdm.json`) holding the segment table and metadata
//! needed to resume after a crash or restart.

use std::path::Path;

use crate::error::Result;
use crate::types::DownloadTask;
use crate::util::sidecar_path;

/// Write (or overwrite) the sidecar for a task.
pub fn save(task: &DownloadTask) -> Result<()> {
    let path = sidecar_path(&task.output_path());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(task)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a task from an explicit sidecar path.
pub fn load(path: &Path) -> Result<DownloadTask> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

/// Remove a task's sidecar, ignoring a missing file.
pub fn remove(task: &DownloadTask) -> Result<()> {
    let path = sidecar_path(&task.output_path());
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Category, DownloadSpec, DownloadStatus};

    fn sample(dir: &Path) -> DownloadTask {
        DownloadTask {
            id: "abc".into(),
            spec: DownloadSpec::new("https://example.com/a.zip", dir).file_name("a.zip"),
            status: DownloadStatus::Paused,
            remote: None,
            segments: Vec::new(),
            downloaded: 0,
            total: None,
            speed: 0.0,
            eta_secs: None,
            category: Category::Compressed,
            error: None,
            created_at: 0,
            completed_at: None,
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let task = sample(dir.path());
        save(&task).unwrap();

        let path = sidecar_path(&task.output_path());
        assert!(path.exists(), "sidecar should be written");

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.id, task.id);
        assert_eq!(loaded.spec.url, task.spec.url);
        assert_eq!(loaded.status, task.status);

        remove(&task).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn removing_missing_sidecar_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        assert!(remove(&sample(dir.path())).is_ok());
    }
}
