use dioxus::prelude::*;
use rdm_core::format::{human_bytes, human_eta, human_speed};
use rdm_core::{Category, DownloadStatus, DownloadTask};
use tw_merge::tw_merge;

use crate::components::icons::{Icon, IconName};
use crate::components::RowMenu;
use crate::i18n::Tr;
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
                span { Tr { k: "table.file" } }
                span { class: "text-right", Tr { k: "table.size" } }
                span { Tr { k: "table.progress" } }
                span { class: "text-right", Tr { k: "table.speed" } }
                span { class: "text-right", Tr { k: "table.status" } }
                span { class: "text-right", Tr { k: "table.actions" } }
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
    let selected = state.selected.read().clone();
    let is_selected = selected.as_deref() == Some(id.as_str());

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

    let (badge_key, badge_variant) = status_badge(status);
    let segments = task.segments.clone();
    let percent = format!("{:.0}%", progress * 100.0);
    let icon_class = tw_merge!("size-4", category_color(task.category));
    let done = status == DownloadStatus::Completed;

    let select_id = id.clone();
    let resume_id = id.clone();
    let pause_id = id.clone();
    let stop_id = id.clone();
    let remove_id = id.clone();
    let folder_path = task.output_path();
    let open_file_path = folder_path.clone();
    let menu_task = task.clone();
    let menu = use_signal(|| None::<(f64, f64)>);

    let row_class = tw_merge!(
        "grid grid-cols-[minmax(0,1fr)_6rem_9rem_7rem_6.5rem_5.5rem] items-center gap-3 border-b border-border/60 px-4 py-2 text-sm",
        if is_selected { "bg-primary/10" } else { "hover:bg-muted/40" }
    );

    rsx! {
        div {
            class: row_class,
            onclick: move |_| state.select(Some(select_id.clone())),
            ondoubleclick: move |_| {
                if done {
                    state.open_file(open_file_path.clone());
                }
            },
            oncontextmenu: move |event| {
                event.prevent_default();
                let point = event.client_coordinates();
                let mut menu = menu;
                menu.set(Some((point.x, point.y)));
            },

            div { class: "flex min-w-0 items-center gap-2.5",
                Icon { name: category_icon(task.category), class: icon_class }
                div { class: "min-w-0",
                    div { class: "truncate font-medium", "{name}" }
                    div { class: "truncate text-xs text-muted-foreground", "{url}" }
                }
            }

            div { class: "text-right text-xs tabular-nums text-muted-foreground", "{size}" }

            div { class: "flex flex-col gap-1",
                Progress { value: progress, indeterminate }
                div { class: "flex items-center gap-2 text-[11px] text-muted-foreground",
                    span { class: "tabular-nums", "{percent}" }
                    span { class: "ml-auto tabular-nums", "{eta}" }
                }
                if !segments.is_empty() {
                    div { class: "flex gap-px",
                        for (index, segment) in segments.iter().take(16).enumerate() {
                            div {
                                key: "{index}",
                                class: "h-0.5 flex-1 overflow-hidden rounded-full bg-muted",
                                div {
                                    class: "h-full bg-primary/70",
                                    style: "width: {segment_fill(segment.downloaded, segment.len()):.0}%",
                                }
                            }
                        }
                    }
                }
            }

            div { class: "text-right text-xs tabular-nums", "{speed}" }

            div { class: "text-right",
                Badge { variant: badge_variant, Tr { k: badge_key } }
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
                if done {
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7", onclick: move |_| state.open_folder(folder_path.clone()),
                        Icon { name: IconName::Folder }
                    }
                } else {
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7", onclick: move |_| state.cancel(stop_id.clone()),
                        Icon { name: IconName::Stop }
                    }
                }
                Button { variant: ButtonVariant::Ghost, size: ButtonSize::Icon, class: "size-7 hover:text-destructive", onclick: move |_| state.remove(remove_id.clone()),
                    Icon { name: IconName::Trash }
                }
            }

            RowMenu { state, task: menu_task, pos: menu }
        }
    }
}

#[component]
fn Empty() -> Element {
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-3 p-10 text-center",
            div { class: "grid size-14 place-items-center rounded-2xl bg-primary/15 text-primary",
                Icon { name: IconName::Download, class: "size-7" }
            }
            p { class: "text-sm font-medium text-foreground", Tr { k: "table.empty_title" } }
            p { class: "text-xs text-muted-foreground", Tr { k: "table.empty_hint" } }
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
        DownloadStatus::Queued => ("dl.queued", BadgeVariant::Secondary),
        DownloadStatus::Probing => ("dl.probing", BadgeVariant::Outline),
        DownloadStatus::Downloading => ("dl.downloading", BadgeVariant::Info),
        DownloadStatus::Paused => ("dl.paused", BadgeVariant::Warning),
        DownloadStatus::Completed => ("dl.done", BadgeVariant::Success),
        DownloadStatus::Failed => ("dl.failed", BadgeVariant::Destructive),
        DownloadStatus::Cancelled => ("dl.cancelled", BadgeVariant::Outline),
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

/// A hue per category — the only place the UI spends colour on "data".
fn category_color(category: Category) -> &'static str {
    match category {
        Category::Compressed => "text-amber-400",
        Category::Documents => "text-sky-400",
        Category::Music => "text-fuchsia-400",
        Category::Programs => "text-emerald-400",
        Category::Video => "text-rose-400",
        Category::Images => "text-violet-400",
        Category::Other => "text-slate-400",
    }
}
