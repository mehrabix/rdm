use dioxus::prelude::*;

use crate::i18n::{use_t, Tr};
use crate::settings::{use_settings, OfferSource};
use crate::state::{engine, Downloads, PendingDownload};
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::separator::Separator;

use rdm_core::{FoundKind, GrabOptions};

/// Split a whitespace/comma separated list of glob patterns.
fn patterns(text: &str) -> Vec<String> {
    text.split([' ', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// The category label a grabbed link maps onto (reuses the sidebar's keys).
fn kind_key(kind: FoundKind) -> &'static str {
    match kind {
        FoundKind::Video => "sidebar.video",
        FoundKind::Audio => "sidebar.music",
        FoundKind::Image => "sidebar.images",
        FoundKind::Archive => "sidebar.compressed",
        FoundKind::Document => "sidebar.documents",
        FoundKind::Page | FoundKind::Other => "sidebar.other",
    }
}

#[component]
pub fn GrabDialog(state: Downloads) -> Element {
    let t = use_t();
    let settings = use_settings();

    let mut url = use_signal(String::new);
    let mut depth = use_signal(|| 1u8);
    let mut include = use_signal(String::new);
    let mut exclude = use_signal(String::new);
    let mut same_origin = use_signal(|| true);

    let results = state.grab_results;
    let selected = state.grab_selected;
    let busy = state.grab_busy;

    let scan = move |_| {
        let value = url.read().trim().to_string();
        if value.is_empty() {
            state.grab_error.set(Some(t("grab.error_url")));
            return;
        }
        let options = GrabOptions {
            depth: *depth.read(),
            same_origin: *same_origin.read(),
            include: patterns(&include.read()),
            exclude: patterns(&exclude.read()),
            ..GrabOptions::default()
        };
        state.grab_busy.set(true);
        state.grab_error.set(None);
        spawn(async move {
            match engine().grab_site(&value, options).await {
                Ok(found) => {
                    state.grab_results.set(found);
                    // Everything found is ticked by default: the common case is
                    // "grab this gallery", and unticking is quicker than not.
                    state.select_all_grab(true);
                }
                Err(err) => {
                    state.grab_results.set(Vec::new());
                    state.grab_error.set(Some(err.to_string()));
                }
            }
            state.grab_busy.set(false);
        });
    };

    let download = move |_| {
        let current = settings.get();
        let urls = selected.read().clone();
        // Queue from this task, then close: see `offer_now`.
        spawn(async move {
            for url in urls {
                let mut pending = PendingDownload::new(url, &current, OfferSource::Grabber);
                pending.dir = current.download_dir.clone();
                state.offer_now(pending, &current).await;
            }
            state.close_grab();
        });
    };

    let found_count = results.read().len();
    let selected_count = selected.read().len();

    rsx! {
        Dialog { class: "max-w-2xl", onclose: move |_| state.close_grab(),
            div { class: "mb-4 flex flex-col gap-1",
                h2 { class: "text-base font-semibold", Tr { k: "grab.title" } }
                p { class: "text-xs text-muted-foreground", Tr { k: "grab.subtitle" } }
            }

            div { class: "flex flex-col gap-3",
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-xs font-medium text-muted-foreground", Tr { k: "grab.url" } }
                    div { class: "flex gap-2",
                        Input {
                            placeholder: t("grab.url_placeholder"),
                            value: url,
                            oninput: move |value| url.set(value),
                        }
                        Button { disabled: busy(), onclick: scan,
                            if busy() { Tr { k: "grab.scanning" } } else { Tr { k: "grab.scan" } }
                        }
                    }
                }

                div { class: "flex flex-wrap items-end gap-4",
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "grab.depth" } }
                        input {
                            r#type: "number",
                            min: "0",
                            max: "5",
                            class: "h-9 w-20 rounded-md border border-input bg-transparent px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                            value: "{depth}",
                            oninput: move |event| {
                                if let Ok(value) = event.value().parse::<u8>() {
                                    depth.set(value.min(5));
                                }
                            },
                        }
                    }
                    label { class: "flex items-center gap-2 pb-2.5 text-xs text-muted-foreground",
                        input {
                            r#type: "checkbox",
                            checked: same_origin(),
                            onchange: move |event| same_origin.set(event.checked()),
                        }
                        Tr { k: "grab.same_origin" }
                    }
                    span { class: "pb-2.5 text-xs text-muted-foreground", Tr { k: "grab.depth_hint" } }
                }

                div { class: "grid grid-cols-2 gap-3",
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "grab.include" } }
                        Input {
                            placeholder: "*.mp4 *.zip",
                            value: include,
                            oninput: move |value| include.set(value),
                        }
                    }
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-xs font-medium text-muted-foreground", Tr { k: "grab.exclude" } }
                        Input {
                            placeholder: "*/ads/*",
                            value: exclude,
                            oninput: move |value| exclude.set(value),
                        }
                    }
                }
                p { class: "-mt-1 text-xs text-muted-foreground", Tr { k: "grab.filters_hint" } }
            }

            Separator { class: "my-4" }

            div { class: "mb-2 flex items-center justify-between gap-3",
                div { class: "text-xs text-muted-foreground",
                    Tr { k: "grab.found" }
                    span { class: "ms-1 font-medium text-foreground", "{found_count}" }
                }
                div { class: "flex gap-2",
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| state.select_all_grab(true),
                        Tr { k: "grab.select_all" }
                    }
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| state.select_all_grab(false),
                        Tr { k: "grab.select_none" }
                    }
                }
            }

            div { class: "max-h-64 overflow-y-auto rounded-md border border-border",
                if found_count == 0 {
                    p { class: "p-4 text-xs text-muted-foreground",
                        if busy() { Tr { k: "grab.scanning" } } else { Tr { k: "grab.empty" } }
                    }
                }
                for item in results.read().iter().cloned() {
                    {
                        let url = item.url.clone();
                        let ticked = selected.read().contains(&url);
                        rsx! {
                            label {
                                key: "{item.url}",
                                class: "flex items-center gap-2 border-b border-border px-3 py-2 text-xs last:border-0 hover:bg-muted/50",
                                input {
                                    r#type: "checkbox",
                                    checked: ticked,
                                    onchange: move |_| state.toggle_grab(url.clone()),
                                }
                                span { class: "shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground",
                                    Tr { k: kind_key(item.kind) }
                                }
                                span { class: "truncate", title: "{item.url}", "{item.url}" }
                            }
                        }
                    }
                }
            }

            if let Some(message) = state.grab_error.read().clone() {
                p { class: "mt-2 text-xs text-destructive", "{message}" }
            }

            Separator { class: "my-4" }

            div { class: "flex items-center justify-between gap-3",
                span { class: "text-xs text-muted-foreground", "{selected_count}" }
                div { class: "flex gap-2",
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| state.close_grab(),
                        Tr { k: "options.close" }
                    }
                    Button {
                        disabled: selected_count == 0,
                        onclick: download,
                        Tr { k: "grab.download" }
                    }
                }
            }
        }
    }
}
