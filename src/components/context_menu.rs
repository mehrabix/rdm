//! Right-click context menu for a transfer row.

use dioxus::prelude::*;
use rdm_core::{DownloadStatus, DownloadTask};

use crate::i18n::use_t;
use crate::state::Downloads;

/// Rendered by a row when it is right-clicked (`pos` is the click position).
#[component]
pub fn RowMenu(state: Downloads, task: DownloadTask, pos: Signal<Option<(f64, f64)>>) -> Element {
    let t = use_t();
    let Some((x, y)) = *pos.read() else {
        return rsx! {};
    };

    let id = task.id.clone();
    let path = task.output_path();
    let url = task.spec.url.clone();
    let done = task.status == DownloadStatus::Completed;
    let running = task.status.is_active() || task.status == DownloadStatus::Queued;
    let resumable = matches!(
        task.status,
        DownloadStatus::Paused | DownloadStatus::Failed | DownloadStatus::Cancelled
    );

    rsx! {
        // click-away backdrop
        div {
            class: "fixed inset-0 z-50",
            onclick: move |_| close(pos),
            oncontextmenu: move |event| {
                event.prevent_default();
                close(pos);
            },
        }

        div {
            class: "fixed z-[60] min-w-52 rounded-md border border-border bg-card p-1 text-xs shadow-2xl",
            style: "left: {x}px; top: {y}px;",
            onclick: move |event| event.stop_propagation(),

            if done {
                MenuRow {
                    label: t("menu.downloads.open"),
                    onclick: {
                        let state = state;
                        let path = path.clone();
                        move |_| { state.open_file(path.clone()); close(pos); }
                    },
                }
            }
            MenuRow {
                label: t("menu.downloads.open_folder"),
                onclick: {
                    let state = state;
                    let path = path.clone();
                    move |_| { state.open_folder(path.clone()); close(pos); }
                },
            }
            MenuRow {
                label: t("menu.downloads.copy_url"),
                onclick: move |_| {
                    crate::state::copy_to_clipboard(&url);
                    close(pos);
                },
            }
            Divider {}
            if running {
                MenuRow {
                    label: t("menu.downloads.pause"),
                    onclick: {
                        let state = state;
                        let id = id.clone();
                        move |_| { state.pause(id.clone()); close(pos); }
                    },
                }
            } else if resumable {
                MenuRow {
                    label: t("menu.downloads.resume"),
                    onclick: {
                        let state = state;
                        let id = id.clone();
                        move |_| { state.resume(id.clone()); close(pos); }
                    },
                }
            }
            MenuRow {
                label: t("menu.downloads.remove"),
                onclick: {
                    let state = state;
                    let id = id.clone();
                    move |_| { state.remove(id.clone()); close(pos); }
                },
            }
        }
    }
}

fn close(pos: Signal<Option<(f64, f64)>>) {
    let mut pos = pos;
    pos.set(None);
}

#[component]
fn MenuRow(label: String, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "flex w-full items-center rounded px-2 py-1 text-start hover:bg-accent",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

#[component]
fn Divider() -> Element {
    rsx! {
        div { class: "my-1 h-px bg-border" }
    }
}

