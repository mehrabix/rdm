//! Batch import: paste many URLs (or the path to a `.txt` file) at once.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::i18n::{use_t, Tr};
use crate::settings::{use_settings, OfferSource};
use crate::state::{Downloads, PendingDownload};
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::separator::Separator;

#[component]
pub fn BatchDialog(state: Downloads) -> Element {
    let t = use_t();
    let settings = use_settings();

    let mut text = use_signal(String::new);
    let mut dir = use_signal(|| settings.get().download_dir.to_string_lossy().to_string());
    let mut error = use_signal(|| None::<String>);

    let placeholder = t("batch.placeholder");
    let none = t("batch.none");
    let text_value = text.read().clone();
    let dir_value = dir.read().clone();

    let close = move |_| {
        let mut open = state.show_batch;
        open.set(false);
    };

    let submit = move |_| {
        let raw = text.read().clone();
        let destination = PathBuf::from(dir.read().clone());
        let current = settings.get();
        let urls = Downloads::urls_in(&raw);
        if urls.is_empty() {
            error.set(Some(none.clone()));
            return;
        }
        settings.update(|s| s.download_dir = destination.clone());

        let mut queue = Vec::new();
        for url in urls {
            let mut pending = PendingDownload::new(url, &current, OfferSource::Batch);
            pending.dir = destination.clone();
            queue.push(pending);
        }
        // Queue everything from *this* task and close once it is done: a dialog
        // that closes while the queueing is still pending would have its task
        // cancelled along with the component.
        spawn(async move {
            for pending in queue {
                state.offer_now(pending, &current).await;
            }
            state.close_batch();
        });
    };

    rsx! {
        Dialog { class: "max-w-xl", onclose: close,
            div { class: "mb-3 flex flex-col gap-1",
                h2 { class: "text-base font-semibold", Tr { k: "batch.title" } }
                p { class: "text-xs text-muted-foreground", Tr { k: "batch.subtitle" } }
            }

            div { class: "flex flex-col gap-3",
                textarea {
                    class: "h-40 w-full resize-none rounded-md border border-input bg-transparent p-3 font-mono text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                    placeholder,
                    value: text_value,
                    oninput: move |event| text.set(event.value()),
                }
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", Tr { k: "add.folder" } }
                    Input { value: dir_value, oninput: move |value| dir.set(value) }
                }
                if let Some(message) = error.read().clone() {
                    p { class: "text-xs text-destructive", "{message}" }
                }
            }

            Separator { class: "my-4" }

            div { class: "flex justify-end gap-2",
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |_| {
                        let mut open = state.show_batch;
                        open.set(false);
                    },
                    Tr { k: "add.cancel" }
                }
                Button { onclick: submit, Tr { k: "batch.queue" } }
            }
        }
    }
}
