use std::path::PathBuf;

use dioxus::prelude::*;

use crate::i18n::{use_t, Tr};
use crate::settings::use_settings;
use crate::state::Downloads;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::separator::Separator;

#[component]
pub fn AddDialog(state: Downloads) -> Element {
    let t = use_t();
    let settings = use_settings();

    let mut url = use_signal(String::new);
    let mut dir = use_signal(|| settings.get().download_dir.to_string_lossy().to_string());
    let mut error = use_signal(|| None::<String>);
    let connections = state.connections;

    let placeholder = t("add.url_placeholder");
    let hint = t("add.connections_hint");
    let error_no_url = t("add.error_no_url");

    let submit = move |_| {
        tracing::info!("add dialog: Start clicked");
        let value = url.read().trim().to_string();
        if value.is_empty() {
            error.set(Some(error_no_url.clone()));
            return;
        }
        let destination = PathBuf::from(dir.read().clone());
        let organize = settings.get().organize_by_category;
        // Remember the folder for next time, then queue the download on the
        // runtime. The dialog only closes when the download was actually queued.
        settings.update(|s| s.download_dir = destination.clone());
        spawn(async move {
            tracing::info!(%value, "queuing download");
            match state.try_add(value, destination, organize).await {
                Ok(()) => {
                    tracing::info!("queued ok");
                    state.close_add();
                    let mut field = url;
                    field.set(String::new());
                    error.set(None);
                }
                Err(err) => {
                    tracing::error!(%err, "queue failed");
                    error.set(Some(err.to_string()));
                }
            }
        });
    };

    rsx! {
        Dialog { onclose: move |_| state.close_add(),
            div { class: "mb-4 flex flex-col gap-1",
                h2 { class: "text-base font-semibold", Tr { k: "add.title" } }
                p { class: "text-xs text-muted-foreground", Tr { k: "add.subtitle" } }
            }

            div { class: "flex flex-col gap-3",
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", Tr { k: "add.url" } }
                    Input {
                        placeholder,
                        value: url,
                        oninput: move |value| url.set(value),
                    }
                }

                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", Tr { k: "add.folder" } }
                    Input {
                        value: dir,
                        oninput: move |value| dir.set(value),
                    }
                }

                div { class: "flex items-end gap-3",
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "add.connections" } }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "32",
                            class: "h-9 w-24 rounded-md border border-input bg-transparent px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                            value: "{connections}",
                            oninput: move |event| {
                                if let Ok(n) = event.value().parse::<u8>() {
                                    state.set_connections(n);
                                }
                            },
                        }
                    }
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "add.queue" } }
                        crate::ui::select::Select {
                            class: "w-36",
                            value: state.queue.read().clone(),
                            options: settings
                                .get()
                                .queue_names()
                                .into_iter()
                                .map(|name| (name.clone(), name))
                                .collect(),
                            onchange: move |name: String| {
                                let mut queue = state.queue;
                                queue.set(name);
                            },
                        }
                    }
                    p { class: "pb-2 text-xs text-muted-foreground", "{hint}" }
                }

                if let Some(message) = error.read().clone() {
                    p { class: "text-xs text-destructive", "{message}" }
                }
            }

            Separator { class: "my-4" }

            div { class: "flex justify-end gap-2",
                Button { variant: ButtonVariant::Outline, onclick: move |_| state.close_add(), Tr { k: "add.cancel" } }
                Button { onclick: submit, Tr { k: "add.start" } }
            }
        }
    }
}
