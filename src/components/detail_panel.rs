//! Per-download detail panel (IDM's "Properties" / details view).

use dioxus::prelude::*;
use rdm_core::format::{human_bytes, human_eta, human_speed};
use rdm_core::{DownloadStatus, SegmentStatus};

use crate::components::icons::{Icon, IconName};
use crate::i18n::Tr;
use crate::state::{file_name, Downloads};
use crate::ui::badge::{Badge, BadgeVariant};
use crate::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::ui::progress::Progress;
use crate::ui::separator::Separator;

#[component]
pub fn DetailPanel(state: Downloads) -> Element {
    let selected = state.selected.read().clone();
    let Some(id) = selected else {
        return rsx! {};
    };
    let task = state.tasks.read().iter().find(|t| t.id == id).cloned();
    let Some(task) = task else {
        return rsx! {};
    };

    let name = file_name(&task);
    let status = task.status;
    let (badge_key, badge_variant) = status_badge(status);
    let size = task
        .total
        .map(human_bytes)
        .unwrap_or_else(|| "—".to_string());
    let downloaded = human_bytes(task.downloaded);
    let speed = human_speed(task.speed);
    let eta = task
        .total
        .map(|total| human_eta(total.saturating_sub(task.downloaded), task.speed))
        .unwrap_or_else(|| "—".to_string());
    let progress = task.progress().unwrap_or(0.0);

    let output_path = task.output_path().to_string_lossy().to_string();
    let (final_url, content_type, etag, last_modified) = task
        .remote
        .as_ref()
        .map(|remote| {
            (
                remote.final_url.clone(),
                remote.content_type.clone().unwrap_or_else(|| "—".into()),
                remote.etag.clone().unwrap_or_else(|| "—".into()),
                remote.last_modified.clone().unwrap_or_else(|| "—".into()),
            )
        })
        .unwrap_or_else(|| {
            (
                task.spec.url.clone(),
                "—".into(),
                "—".into(),
                "—".into(),
            )
        });

    let segments = task.segments.clone();
    let done = status == DownloadStatus::Completed;
    let folder_path = task.output_path();

    rsx! {
        aside { class: "flex w-80 shrink-0 flex-col gap-3 overflow-y-auto border-l border-border bg-card/40 p-4 text-sm",
            div { class: "flex items-start gap-2",
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-semibold", "{name}" }
                    div { class: "mt-1", Badge { variant: badge_variant, Tr { k: badge_key } } }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    class: "size-7",
                    onclick: move |_| state.select(None),
                    Icon { name: IconName::Stop }
                }
            }

            Progress { value: progress, indeterminate: task.total.is_none() && status.is_active() }

            div { class: "grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs",
                Field { label: "Size", value: size }
                Field { label: "Downloaded", value: downloaded }
                Field { label: "Speed", value: speed }
                Field { label: "Time left", value: eta }
                Field { label: "Segments", value: format!("{}", segments.len()) }
                Field { label: "Category", value: format!("{:?}", task.category) }
            }

            Separator {}

            div { class: "flex flex-col gap-1.5 text-xs",
                Field { label: "URL", value: final_url }
                Field { label: "Content-Type", value: content_type }
                Field { label: "ETag", value: etag }
                Field { label: "Last-Modified", value: last_modified }
                Field { label: "Saved to", value: output_path }
            }

            if !segments.is_empty() {
                Separator {}
                div { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", "Connections" }
                    for segment in segments.iter() {
                        div { class: "flex items-center gap-2 text-[11px]",
                            span { class: "w-6 tabular-nums text-muted-foreground", "#{segment.index}" }
                            Progress { value: segment_fill(segment.downloaded, segment.len()), class: "flex-1" }
                            span { class: "w-20 text-right tabular-nums text-muted-foreground",
                                "{human_bytes(segment.downloaded)}/{human_bytes(segment.len())}"
                            }
                            span { class: "w-16 text-right text-muted-foreground", "{status_label(segment.status)}" }
                        }
                    }
                }
            }

            div { class: "mt-auto flex gap-2",
                if done {
                    Button {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Sm,
                        onclick: move |_| state.open_folder(folder_path.clone()),
                        Icon { name: IconName::Folder }
                        "Open"
                    }
                }
            }
        }
    }
}

#[component]
fn Field(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex min-w-0 flex-col",
            span { class: "text-[10px] uppercase tracking-wide text-muted-foreground", "{label}" }
            span { class: "truncate", title: "{value}", "{value}" }
        }
    }
}

fn segment_fill(downloaded: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (downloaded as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn status_label(status: SegmentStatus) -> &'static str {
    match status {
        SegmentStatus::Pending => "pending",
        SegmentStatus::Active => "active",
        SegmentStatus::Done => "done",
        SegmentStatus::Failed => "failed",
    }
}

fn status_badge(status: DownloadStatus) -> (&'static str, BadgeVariant) {
    match status {
        DownloadStatus::Queued => ("dl.queued", BadgeVariant::Secondary),
        DownloadStatus::Probing => ("dl.probing", BadgeVariant::Outline),
        DownloadStatus::Downloading => ("dl.downloading", BadgeVariant::Info),
        DownloadStatus::Paused => ("dl.paused", BadgeVariant::Warning),
        DownloadStatus::Completed => ("dl.done", BadgeVariant::Success),
        DownloadStatus::Failed => ("dl.failed", BadgeVariant::Destructive),
        DownloadStatus::Cancelled => ("dl.cancelled", BadgeVariant::Outline),
    }
}
