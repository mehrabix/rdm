use std::path::PathBuf;

use dioxus::prelude::*;

use crate::state::Downloads;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::separator::Separator;

#[component]
pub fn AddDialog(state: Downloads) -> Element {
    let mut url = use_signal(String::new);
    let mut dir = use_signal(|| {
        rdm_core::util::default_download_dir()
            .to_string_lossy()
            .to_string()
    });
    let mut error = use_signal(|| None::<String>);
    let connections = state.connections;

    let submit = move |_| {
        let value = url.read().trim().to_string();
        if value.is_empty() {
            error.set(Some("Enter a URL to download.".into()));
            return;
        }
        let destination = PathBuf::from(dir.read().clone());
        state.add(value, destination);
        state.close_add();
        url.set(String::new());
        error.set(None);
    };

    rsx! {
        Dialog { onclose: move |_| state.close_add(),
            div { class: "mb-4 flex flex-col gap-1",
                h2 { class: "text-base font-semibold", "New download" }
                p { class: "text-xs text-muted-foreground", "RDM will probe the URL and split it into parallel connections." }
            }

            div { class: "flex flex-col gap-3",
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", "URL" }
                    Input {
                        placeholder: "https://example.com/big-file.zip",
                        value: url,
                        oninput: move |value| url.set(value),
                    }
                }

                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", "Save to folder" }
                    Input {
                        value: dir,
                        oninput: move |value| dir.set(value),
                    }
                }

                div { class: "flex items-end gap-3",
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", "Connections" }
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
                    p { class: "pb-2 text-xs text-muted-foreground", "1–32. More connections accelerate ranged downloads." }
                }

                if let Some(message) = error.read().clone() {
                    p { class: "text-xs text-destructive", "{message}" }
                }
            }

            Separator { class: "my-4" }

            div { class: "flex justify-end gap-2",
                Button { variant: ButtonVariant::Outline, onclick: move |_| state.close_add(), "Cancel" }
                Button { onclick: submit, "Start download" }
            }
        }
    }
}
