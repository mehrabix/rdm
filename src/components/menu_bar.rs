//! In-app menu bar (File / Tasks / Downloads / View / Tools / Help).
//!
//! Rendered as real HTML — the framework's native bar was non-functional and is
//! disabled in `main.rs`.

use dioxus::prelude::*;

use crate::i18n::use_t;
use crate::settings::{use_settings, Theme};
use crate::state::Downloads;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    File,
    Tasks,
    Downloads,
    View,
    Tools,
    Help,
}

#[component]
pub fn MenuBar(state: Downloads) -> Element {
    let t = use_t();
    let open = use_signal(|| None::<Menu>);

    rsx! {
        div { class: "relative flex items-center gap-0.5 border-b border-border bg-card px-2 py-0.5 text-xs",

            MenuButton { open, menu: Menu::File, label: t("menu.file"),
                Item { label: t("menu.file.new"), onclick: { let state = state; let open = open; move |_| { state.open_add(); close(open); } } }
                Item { label: t("menu.file.batch"), onclick: {
                    let state = state;
                    let open = open;
                    move |_| { let mut b = state.show_batch; b.set(true); close(open); }
                } }
                Item { label: t("menu.file.import_clipboard"), onclick: { let state = state; let open = open; move |_| { state.import_clipboard(); close(open); } } }
                Sep {}
                Item { label: t("menu.file.exit"), onclick: move |_| exit_app() }
            }

            MenuButton { open, menu: Menu::Tasks, label: t("menu.tasks"),
                Item { label: t("menu.tasks.start_queue"), onclick: { let state = state; let open = open; move |_| { state.resume_all(); close(open); } } }
                Item { label: t("menu.tasks.stop_queue"), onclick: { let state = state; let open = open; move |_| { state.pause_all(); close(open); } } }
                Sep {}
                Item { label: t("menu.tasks.scheduler"), disabled: true, onclick: move |_| {} }
                Item { label: t("menu.tasks.grabber"), disabled: true, onclick: move |_| {} }
            }

            MenuButton { open, menu: Menu::Downloads, label: t("menu.downloads"),
                Item { label: t("menu.downloads.resume"), onclick: { let state = state; let open = open; move |_| { resume_selected(state); close(open); } } }
                Item { label: t("menu.downloads.pause"), onclick: { let state = state; let open = open; move |_| { pause_selected(state); close(open); } } }
                Sep {}
                Item { label: t("menu.downloads.resume_all"), onclick: { let state = state; let open = open; move |_| { state.resume_all(); close(open); } } }
                Item { label: t("menu.downloads.pause_all"), onclick: { let state = state; let open = open; move |_| { state.pause_all(); close(open); } } }
                Sep {}
                Item { label: t("menu.downloads.remove_completed"), onclick: { let state = state; let open = open; move |_| { state.remove_completed(); close(open); } } }
            }

            MenuButton { open, menu: Menu::View, label: t("menu.view"),
                ThemeItem { open, theme: Theme::Light, label: t("theme.light") }
                ThemeItem { open, theme: Theme::Dark, label: t("theme.dark") }
                ThemeItem { open, theme: Theme::System, label: t("theme.system") }
                Sep {}
                Item { label: t("menu.view.language"), onclick: { let state = state; let open = open; move |_| { state.open_options(); close(open); } } }
            }

            MenuButton { open, menu: Menu::Tools, label: t("menu.tools"),
                Item { label: t("menu.tools.options"), onclick: { let state = state; let open = open; move |_| { state.open_options(); close(open); } } }
                Sep {}
                Item { label: t("menu.tools.speed_limiter"), disabled: true, onclick: move |_| {} }
                Item { label: t("menu.tools.updates"), disabled: true, onclick: move |_| {} }
            }

            MenuButton { open, menu: Menu::Help, label: t("menu.help"),
                Item { label: t("menu.help.about"), onclick: { let state = state; let open = open; move |_| { state.open_options(); close(open); } } }
                Item { label: t("menu.help.docs"), disabled: true, onclick: move |_| {} }
            }

            // Click-away backdrop while a menu is open.
            if open.read().is_some() {
                div { class: "fixed inset-0 z-40", onclick: move |_| close(open) }
            }
        }
    }
}

fn close(open: Signal<Option<Menu>>) {
    let mut open = open;
    open.set(None);
}

/// Exit the process. Wrapped in a `()`-returning fn because a closure whose body
/// diverges infers a `!` return type, which Dioxus' event handler rejects.
fn exit_app() {
    std::process::exit(0);
}

fn resume_selected(state: Downloads) {
    for id in state.selected_ids() {
        state.resume(id);
    }
}

fn pause_selected(state: Downloads) {
    for id in state.selected_ids() {
        state.pause(id);
    }
}

#[component]
fn MenuButton(
    open: Signal<Option<Menu>>,
    menu: Menu,
    label: String,
    children: Element,
) -> Element {
    let is_open = *open.read() == Some(menu);
    let class = if is_open {
        "rounded px-2 py-0.5 bg-accent text-accent-foreground"
    } else {
        "rounded px-2 py-0.5 hover:bg-accent/60"
    };

    rsx! {
        div { class: "relative",
            button {
                class,
                onclick: move |_| {
                    let mut open = open;
                    open.set(if is_open { None } else { Some(menu) });
                },
                "{label}"
            }
            if is_open {
                div {
                    class: "absolute left-0 top-full z-50 mt-1 min-w-56 rounded-md border border-border bg-card p-1 shadow-xl",
                    onclick: move |event| event.stop_propagation(),
                    {children}
                }
            }
        }
    }
}

#[component]
fn Item(label: String, #[props(default)] disabled: bool, onclick: EventHandler<()>) -> Element {
    let class = if disabled {
        "flex w-full items-center rounded px-2 py-1 text-left text-muted-foreground/50"
    } else {
        "flex w-full items-center rounded px-2 py-1 text-left hover:bg-accent"
    };
    rsx! {
        button {
            class,
            disabled,
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

#[component]
fn ThemeItem(open: Signal<Option<Menu>>, theme: Theme, label: String) -> Element {
    let settings = use_settings();
    let active = settings.get().theme == theme;
    let prefix = if active { "✓ " } else { "   " };

    rsx! {
        Item {
            label: "{prefix}{label}",
            onclick: move |_| {
                settings.update(|s| s.theme = theme);
                close(open);
            },
        }
    }
}

#[component]
fn Sep() -> Element {
    rsx! {
        div { class: "my-1 h-px bg-border" }
    }
}
