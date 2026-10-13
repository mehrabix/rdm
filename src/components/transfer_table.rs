use dioxus::prelude::*;
use rdm_core::format::{human_bytes, human_eta, human_speed};
use rdm_core::{Category, DownloadStatus, DownloadTask};

use crate::components::icons::{Icon, IconName};
use crate::state::{file_name, Downloads};
use crate::ui::badge::{Badge, BadgeVariant};
use crate::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::ui::progress::Progress;

#[component]
pub fn TransferTable(state: Downloads) -> Element {
    let rows = state.visible();

    rsx! {
        main { class: "flex min-w-0 flex-1 flex-col",
            div { class: "grid grid-cols-[minmax(0,1fr)_6rem_9rem_7rem_6.5rem_5.5rem] items-center gap-3 border-b border-border bg-muted/40 px-4 py-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground",
                span { "File" }
                span { class: "text-right", "Size" }
                span { "Progress" }
                span { class: "text-right", "Speed" }
                span { class: "text-right", "Status" }
                span { class: "text-right", "Actions" }
            }

            div { class: "min-h-0 flex-1 overflow-y-auto",
                if rows.is_empty() {
                    Empty {}
                } else {
                    for task in rows {
                        Row { key: "{task.id}", state, task }
                    }
                }
            }
        }
    }
}

#[component]
fn Row(state: Downloads, task: DownloadTask) -> Element {
    let id = task.id.clone();
    let name = file_name(&task);
    let url = task
        .remote
        .as_ref()
        .map(|remote| remote.final_url.clone())
        .unwrap_or_else(|| task.spec.url.clone());
    let status = task.status;
    let progress = task.progress().unwrap_or(0.0);
    let indeterminate = task.total.is_none() && status.is_active();
    let size = task
        .total
        .map(human_bytes)
        .unwrap_or_else(|| human_bytes(task.downloaded));
    let speed = if status.is_active() {
        human_speed(task.speed)
    } else {
        "—".to_string()
    };
    let eta = task
        .total
        .map(|total| human_eta(total.saturating_sub(task.downloaded), task.speed))
        .unwrap_or_else(|| "—".to_string());

    let resume_id = id.clone();
    let pause_id = id.clone();
    let stop_id = id.clone();
    let remove_id = id.clone();

    let (badge_label, badge_variant) = status_badge(status);
    let segments = task.segments.clone();

    rsx! {
        div { class: "grid grid-cols-[minmax(0,1fr)_6rem_9rem_7rem_6.5rem_5.5rem] items-center gap-3 border-b border-border/60 px-4 py-2 text-sm hover:bg-accent/30",
            div { class: "flex min-w-0 items-center gap-2.5",
                Icon { name: category_icon(task.category), class: "size-4 text-muted-foreground" }
                div { class: "min-w-0",
                    div { class: "truncate font-medium", "{name}" }
                    div { class: "truncate text-xs text-muted-foreground", "{url}" }
                }
            }

            div { class: "text-right text-xs tabular-nums text-muted-foreground", "{size}" }

            div { class: "flex flex-col gap-1",
                Progress { value: progress, indeterminate }
                div { class: "flex items-center gap-2 text-[11px] text-muted-foreground",
                    span { class: "tabular-nums", "{format!(\"{:.0}%\", progress * 100.0)}" }
                    span { class: "ml-auto tabular-nums", "{eta}" }
                }
                if !segments.is_empty() {
                    div { class: "flex gap-px",
                        for (index, segment) in segments.iter().take(16).enumerate() {
                            div {
                                key: "{index}",
                                class: "h-0.5 flex-1 overflow-hidden rounded-full bg-secondary",
                                div {
                                    class: "h-full bg-primary/80",
                                    style: format!("width: {:.0}%", segment_fill(segment.downloaded, segment.len())),
                                }
                            }
                        }
                    }
                }
            }

            div { class: "text-right text-xs tabular-nums", "{speed}" }

            div { class: "text-right",
                Badge { variant: badge_variant, "{badge_label}" }
            }

            div { class: "flex items-center justify-end gap-1",
                if status == DownloadStatus::Downloading || status == DownloadStatus::Queued {
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7", onclick: move |_| state.pause(pause_id.clone()),
                        Icon { name: IconName::Pause }
                    }
                } else if status == DownloadStatus::Paused || status == DownloadStatus::Failed || status == DownloadStatus::Cancelled {
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7", onclick: move |_| state.resume(resume_id.clone()),
                        Icon { name: IconName::Play }
                    }
                }
                Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7", onclick: move |_| state.cancel(stop_id.clone()),
                    Icon { name: IconName::Stop }
                }
                Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7 hover:text-destructive", onclick: move |_| state.remove(remove_id.clone()),
                    Icon { name: IconName::Trash }
                }
            }
        }
    }
}

#[component]
fn Empty() -> Element {
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-2 p-10 text-center text-muted-foreground",
            Icon { name: IconName::Download, class: "size-9 opacity-40" }
            p { class: "text-sm font-medium text-foreground", "No downloads here yet" }
            p { class: "text-xs", "Click “New” to paste a URL and start downloading." }
        }
    }
}

fn segment_fill(downloaded: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

fn status_badge(status: DownloadStatus) -> (&'static str, BadgeVariant) {
    match status {
        DownloadStatus::Queued => ("Queued", BadgeVariant::Secondary),
        DownloadStatus::Probing => ("Probing", BadgeVariant::Outline),
        DownloadStatus::Downloading => ("Downloading", BadgeVariant::Default),
        DownloadStatus::Paused => ("Paused", BadgeVariant::Outline),
        DownloadStatus::Completed => ("Done", BadgeVariant::Success),
        DownloadStatus::Failed => ("Failed", BadgeVariant::Destructive),
        DownloadStatus::Cancelled => ("Cancelled", BadgeVariant::Outline),
    }
}

fn category_icon(category: Category) -> IconName {
    match category {
        Category::Compressed => IconName::Archive,
        Category::Documents => IconName::FileText,
        Category::Music => IconName::Music,
        Category::Programs => IconName::Package,
        Category::Video => IconName::Video,
        Category::Images => IconName::Image,
        Category::Other => IconName::File,
    }
}
