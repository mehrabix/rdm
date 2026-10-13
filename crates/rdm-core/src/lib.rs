//! `rdm-core` is the headless, UI-agnostic download engine behind **RDM**
//! (Rust Download Manager).
//!
//! It implements the parts that make a download manager more than `curl`:
//! HTTP range probing, multi-connection segmentation, resumable transfers,
//! live speed/ETA accounting and a small scheduler-friendly manager API.
//!
//! The crate has no dependency on any GUI toolkit, so it can be unit tested
//! (and even embedded in a CLI) without a display server.

pub mod error;
pub mod format;
pub mod http;
pub mod rate;
pub mod segment;
pub mod speed;
pub mod store;
pub mod types;
pub mod util;

mod engine;

pub use engine::{DownloadManager, EngineConfig, EngineEvent};
pub use error::{Error, Result};
pub use rate::RateLimiter;
pub use types::{
    Category, DownloadSpec, DownloadStatus, DownloadTask, ProgressUpdate, RemoteInfo, Segment,
    SegmentStatus,
};
