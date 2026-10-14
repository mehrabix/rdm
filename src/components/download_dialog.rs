use std::path::PathBuf;

use dioxus::prelude::*;

use crate::i18n::{use_t, Tr};
use crate::settings::{use_settings, SettingsCtx};
use crate::state::{engine, Downloads, PendingDownload};
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::select::Select;
use crate::ui::separator::Separator;

use rdm_core::format::human_bytes;

/// How long the window waits for the size/type probe before showing what it has.
/// The download is never blocked by this — Start works immediately.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// IDM's "Download File Info": everything about a recognized download, with
/// **Start Download** / **Download Later** / **Cancel**. Nothing transfers until
/// one of those is pressed.
#[component]
pub fn DownloadDialog(state: Downloads) -> Element {
    let t = use_t();
    let settings = use_settings();

    let Some(snapshot) = state.pending.read().clone() else {
        return rsx! {};
    };

    let derived = snapshot.suggested_name();
    let mut url = use_signal(|| snapshot.url.clone());
    let mut file_name = use_signal(|| derived.clone());
    let mut dir = use_signal(|| snapshot.dir.to_string_lossy().to_string());
    let mut queue = use_signal(|| snapshot.queue.clone());
    let mut connections = use_signal(|| snapshot.connections.max(1));
    let mut dont_ask = use_signal(|| false);
    /// Set once the user edits the name, so an extractor keeps it.
    let mut keep_name = use_signal(|| false);
    let mut probed = use_signal(|| None::<rdm_core::RemoteInfo>);
    let mut checking = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    // Which offer the local fields were filled from. Deliberately *not* a
    // signal: a reactive one would make the effect below re-run itself.
    let applied = use_hook(|| std::cell::RefCell::new(String::new()));

    // Fill the window for each newly offered download, and probe it so the size
    // and type are real rather than guessed.
    use_effect(move || {
        let Some(pending) = state.pending.read().clone() else {
            return;
        };
        let key = format!("{}|{}", pending.url, pending.source.key());
        if *applied.borrow() == key {
            return;
        }
        *applied.borrow_mut() = key;

        let derived = rdm_core::util::filename_from_url(&pending.url);
        url.set(pending.url.clone());
        file_name.set(pending.file_name.clone().unwrap_or_else(|| derived.clone()));
        dir.set(pending.dir.to_string_lossy().to_string());
        queue.set(pending.queue.clone());
        connections.set(pending.connections.max(1));
        probed.set(None);
        error.set(None);
        checking.set(true);

        let spec = pending.spec(false, false);
        spawn(async move {
            let result =
                tokio::time::timeout(PROBE_TIMEOUT, engine().probe(&spec)).await;
            checking.set(false);
            if let Ok(Ok(info)) = result {
                // Refine the name only while it is still the derived one, so a
                // server-supplied name never overwrites the user's typing.
                if !info.file_name.trim().is_empty() {
                    let current = file_name.peek().clone();
                    if current.is_empty() || current == derived {
                        file_name.set(info.file_name.clone());
                    }
                }
                probed.set(Some(info));
            }
        });
    });

    // One clone per button: a `move` closure can only take the value once.
    let start_snapshot = snapshot.clone();
    let later_snapshot = snapshot.clone();
    let start = move |_| match prepare(
        &start_snapshot,
        settings,
        *dont_ask.read(),
        url.read().clone(),
        file_name.read().clone(),
        dir.read().clone(),
        queue.read().clone(),
        *connections.read(),
        *keep_name.read(),
        &t("dlg.info.error_url"),
    ) {
        Ok((value, organize)) => {
            let state = state;
            spawn(async move {
                if let Err(err) = state.start_pending(value, organize, false).await {
                    error.set(Some(err.to_string()));
                }
            });
        }
        Err(message) => error.set(Some(message)),
    };

    let later = move |_| match prepare(
        &later_snapshot,
        settings,
        *dont_ask.read(),
        url.read().clone(),
        file_name.read().clone(),
        dir.read().clone(),
        queue.read().clone(),
        *connections.read(),
        *keep_name.read(),
        &t("dlg.info.error_url"),
    ) {
        Ok((value, organize)) => {
            let state = state;
            spawn(async move {
                if let Err(err) = state.start_pending(value, organize, true).await {
                    error.set(Some(err.to_string()));
                }
            });
        }
        Err(message) => error.set(Some(message)),
    };

    let size = match (probed.read().as_ref(), checking()) {
        (Some(info), _) => info
            .total_size
            .map(human_bytes)
            .unwrap_or_else(|| "—".to_string()),
        (None, true) => t("dlg.info.checking"),
        (None, false) => "—".to_string(),
    };
    let kind = probed
        .read()
        .as_ref()
        .and_then(|info| info.content_type.clone())
        .unwrap_or_else(|| "—".to_string());
    let category = rdm_core::Category::from_filename(
        &file_name.read().trim().to_string(),
    )
    .folder();

    rsx! {
        Dialog { class: "max-w-xl", onclose: move |_| state.dismiss_pending(),
            div { class: "mb-4 flex flex-col gap-1",
                h2 { class: "text-base font-semibold", Tr { k: "dlg.info.title" } }
                p { class: "text-xs text-muted-foreground", Tr { k: "dlg.info.subtitle" } }
            }

            div { class: "flex flex-col gap-3",
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", Tr { k: "dlg.info.url" } }
                    Input { value: url, oninput: move |value| url.set(value) }
                }

                div { class: "grid grid-cols-2 gap-3",
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "dlg.info.file_name" } }
                        Input {
                            value: file_name,
                            oninput: move |value| {
                                file_name.set(value);
                                keep_name.set(true);
                            },
                        }
                    }
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "dlg.info.save_to" } }
                        Input { value: dir, oninput: move |value| dir.set(value) }
                    }
                }

                div { class: "grid grid-cols-3 gap-3 text-xs",
                    div { class: "flex flex-col gap-1",
                        span { class: "font-medium text-muted-foreground", Tr { k: "dlg.info.size" } }
                        span { class: "truncate text-foreground", "{size}" }
                    }
                    div { class: "flex flex-col gap-1",
                        span { class: "font-medium text-muted-foreground", Tr { k: "dlg.info.type" } }
                        span { class: "truncate text-foreground", "{kind}" }
                    }
                    div { class: "flex flex-col gap-1",
                        span { class: "font-medium text-muted-foreground", Tr { k: "dlg.info.category" } }
                        span { class: "truncate text-foreground", "{category}" }
                    }
                }

                div { class: "flex items-end gap-3",
                    label { class: "flex flex-1 flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "dlg.info.queue" } }
                        Select {
                            value: queue.read().clone(),
                            options: settings
                                .get()
                                .queue_names()
                                .into_iter()
                                .map(|name| (name.clone(), name))
                                .collect(),
                            onchange: move |name: String| queue.set(name),
                        }
                    }
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "dlg.info.connections" } }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "32",
                            class: "h-9 w-24 rounded-md border border-input bg-transparent px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                            value: "{connections}",
                            oninput: move |event| {
                                if let Ok(value) = event.value().parse::<u8>() {
                                    connections.set(value.max(1));
                                }
                            },
                        }
                    }
                }

                label { class: "flex items-center gap-2 text-xs text-muted-foreground",
                    input {
                        r#type: "checkbox",
                        checked: dont_ask(),
                        onchange: move |event| dont_ask.set(event.checked()),
                    }
                    Tr { k: "dlg.info.dont_ask" }
                }

                if let Some(message) = error.read().clone() {
                    p { class: "text-xs text-destructive", "{message}" }
                }
            }

            Separator { class: "my-4" }

            div { class: "flex justify-end gap-2",
                Button { variant: ButtonVariant::Outline, onclick: move |_| state.dismiss_pending(),
                    Tr { k: "dlg.info.cancel" }
                }
                Button { variant: ButtonVariant::Secondary, onclick: later, Tr { k: "dlg.info.later" } }
                Button { onclick: start, Tr { k: "dlg.info.start" } }
            }
        }
    }
}

/// Validate the edited fields, remember the folder for next time, and apply
/// "Don't show this dialog again". Returns the download to queue plus whether
/// downloads are organized into category folders.
#[allow(clippy::too_many_arguments)]
fn prepare(
    snapshot: &PendingDownload,
    settings: SettingsCtx,
    dont_ask: bool,
    url: String,
    file_name: String,
    dir: String,
    queue: String,
    connections: u8,
    keep_name: bool,
    error_url: &str,
) -> Result<(PendingDownload, bool), String> {
    let url = url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(error_url.to_string());
    }

    let mut value = snapshot.clone();
    value.url = url;
    value.dir = PathBuf::from(dir.trim());
    let name = file_name.trim();
    value.file_name = (!name.is_empty()).then(|| name.to_string());
    value.queue = queue;
    value.connections = connections.max(1);
    value.keep_name = keep_name;

    let current = settings.get();
    let organize = current.organize_by_category;
    let destination = value.dir.clone();
    let silence = value.source.key().to_string();
    settings.update(|s| {
        s.download_dir = destination;
        if dont_ask && !s.silent_sources.contains(&silence) {
            s.silent_sources.push(silence);
        }
    });

    Ok((value, organize))
}
