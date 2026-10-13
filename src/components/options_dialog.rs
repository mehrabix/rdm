//! Options dialog — settings, mirroring IDM's tabbed Options.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::i18n::{use_t, LANGUAGES};
use crate::settings::{use_settings, Theme};
use crate::state::Downloads;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::Dialog;
use crate::ui::input::Input;
use crate::ui::separator::Separator;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Downloads,
    Connection,
    Scheduler,
    Categories,
    Browser,
    Sounds,
    About,
}

impl Tab {
    const ALL: [Tab; 8] = [
        Tab::General,
        Tab::Downloads,
        Tab::Connection,
        Tab::Scheduler,
        Tab::Categories,
        Tab::Browser,
        Tab::Sounds,
        Tab::About,
    ];

    fn label_key(self) -> &'static str {
        match self {
            Tab::General => "options.general",
            Tab::Downloads => "options.downloads",
            Tab::Connection => "options.connection",
            Tab::Scheduler => "options.scheduler",
            Tab::Categories => "options.categories",
            Tab::Browser => "options.browser",
            Tab::Sounds => "options.sounds",
            Tab::About => "menu.help.about",
        }
    }
}

#[component]
pub fn OptionsDialog(state: Downloads) -> Element {
    let t = use_t();
    let tab = use_signal(|| Tab::General);
    let title = t("options.title");
    let close_label = t("options.close");
    let current = *tab.read();

    rsx! {
        Dialog { class: "max-w-3xl p-0", onclose: move |_| state.close_options(),
            div { class: "flex min-h-[27rem]",
                nav { class: "flex w-44 shrink-0 flex-col gap-0.5 border-r border-border p-2",
                    for item in Tab::ALL {
                        {
                            let active = current == item;
                            let class = if active {
                                "rounded-md bg-primary/15 px-3 py-1.5 text-start text-sm font-medium text-foreground"
                            } else {
                                "rounded-md px-3 py-1.5 text-start text-sm text-muted-foreground hover:bg-muted hover:text-foreground"
                            };
                            let label = t(item.label_key());
                            rsx! {
                                button {
                                    class,
                                    onclick: move |_| {
                                        let mut tab = tab;
                                        tab.set(item);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                div { class: "flex min-w-0 flex-1 flex-col p-5",
                    h2 { class: "mb-4 text-base font-semibold", "{title}" }

                    div { class: "min-h-0 flex-1 overflow-y-auto",
                        match current {
                            Tab::General => rsx! { GeneralTab {} },
                            Tab::Downloads => rsx! { DownloadsTab {} },
                            Tab::Connection => rsx! { ConnectionTab {} },
                            Tab::Scheduler => rsx! { SchedulerTab {} },
                            Tab::Browser => rsx! { BrowserTab {} },
                            Tab::About => rsx! { AboutTab {} },
                            other => rsx! { StubTab { tab: other } },
                        }
                    }

                    Separator { class: "my-4" }
                    div { class: "flex justify-end",
                        Button { variant: ButtonVariant::Secondary, onclick: move |_| state.close_options(), "{close_label}" }
                    }
                }
            }
        }
    }
}

#[component]
fn GeneralTab() -> Element {
    let t = use_t();
    let settings = use_settings();
    let current = settings.get();

    let theme_label = t("options.general.theme");
    let lang_label = t("options.general.language");
    let folder_label = t("options.general.folder");
    let start_label = t("options.general.start_minimized");
    let confirm_label = t("options.general.confirm_exit");
    let dir_text = current.download_dir.to_string_lossy().to_string();

    rsx! {
        div { class: "flex flex-col gap-5",
            Field { label: theme_label,
                div { class: "flex gap-2",
                    for theme in [Theme::Light, Theme::Dark, Theme::System] {
                        {
                            let active = current.theme == theme;
                            let class = if active {
                                "rounded-md border border-primary bg-primary/15 px-3 py-1 text-sm"
                            } else {
                                "rounded-md border border-border px-3 py-1 text-sm hover:bg-muted"
                            };
                            let label = t(theme.label_key());
                            rsx! {
                                button {
                                    class,
                                    onclick: move |_| settings.update(|s| s.theme = theme),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            Field { label: lang_label,
                crate::ui::select::Select {
                    class: "w-72",
                    value: current.language.clone(),
                    options: LANGUAGES
                        .iter()
                        .map(|(code, name)| (code.to_string(), name.to_string()))
                        .collect(),
                    onchange: move |code: String| settings.update(|s| s.language = code),
                }
            }

            Field { label: folder_label,
                Input {
                    class: "w-full",
                    value: dir_text,
                    oninput: move |value: String| {
                        settings.update(|s| s.download_dir = PathBuf::from(value));
                    },
                }
            }

            Toggle {
                label: t("options.general.organize"),
                checked: current.organize_by_category,
                onchange: move |value: bool| settings.update(|s| s.organize_by_category = value),
            }
            Toggle {
                label: start_label,
                checked: current.start_minimized,
                onchange: move |value: bool| settings.update(|s| s.start_minimized = value),
            }
            Toggle {
                label: confirm_label,
                checked: current.confirm_exit,
                onchange: move |value: bool| settings.update(|s| s.confirm_exit = value),
            }
        }
    }
}

#[component]
fn DownloadsTab() -> Element {
    let t = use_t();
    let settings = use_settings();
    let current = settings.get();

    let connections_label = t("options.downloads.connections");
    let concurrent_label = t("options.downloads.max_concurrent");
    let launch_label = t("options.downloads.launch_on_complete");

    rsx! {
        div { class: "flex flex-col gap-5",
            Field { label: connections_label,
                NumberInput {
                    value: current.connections as i64,
                    min: 1,
                    max: 32,
                    onchange: move |value: i64| settings.update(|s| s.connections = value.clamp(1, 32) as u8),
                }
            }
            Field { label: concurrent_label,
                NumberInput {
                    value: current.max_concurrent as i64,
                    min: 1,
                    max: 16,
                    onchange: move |value: i64| settings.update(|s| s.max_concurrent = value.clamp(1, 16) as u8),
                }
            }
            Toggle {
                label: launch_label,
                checked: current.launch_on_complete,
                onchange: move |value: bool| settings.update(|s| s.launch_on_complete = value),
            }
            Toggle {
                label: t("options.downloads.clipboard_monitor"),
                checked: current.clipboard_monitor,
                onchange: move |value: bool| settings.update(|s| s.clipboard_monitor = value),
            }
        }
    }
}

#[component]
fn ConnectionTab() -> Element {
    let t = use_t();
    let settings = use_settings();
    let current = settings.get();

    let limit_label = t("options.connection.speed_limit");
    let limit_hint = t("options.connection.speed_limit_hint");

    rsx! {
        div { class: "flex flex-col gap-5",
            Field { label: limit_label,
                NumberInput {
                    value: current.speed_limit_kbps as i64,
                    min: 0,
                    max: 1_000_000,
                    onchange: move |value: i64| settings.update(|s| s.speed_limit_kbps = value.max(0) as u32),
                }
            }
            p { class: "text-xs text-muted-foreground", "{limit_hint}" }
        }
    }
}

#[component]
fn BrowserTab() -> Element {
    let t = use_t();
    let settings = use_settings();
    let current = settings.get();
    let hint = t("options.browser.hint");
    rsx! {
        div { class: "flex flex-col gap-5",
            Toggle {
                label: t("options.browser.enable"),
                checked: current.browser_integration,
                onchange: move |value: bool| settings.update(|s| s.browser_integration = value),
            }
            p { class: "text-xs text-muted-foreground", "{hint}" }
        }
    }
}

#[component]
fn AboutTab() -> Element {
    let t = use_t();
    let description = t("about.description");
    rsx! {
        div { class: "flex flex-col gap-2 text-sm",
            p { class: "text-base font-semibold", "RDM — Rust Download Manager" }
            p { class: "text-muted-foreground", "Version 0.1.0" }
            p { class: "text-muted-foreground", "{description}" }
        }
    }
}

#[component]
fn StubTab(tab: Tab) -> Element {
    let t = use_t();
    let label = t(tab.label_key());
    let note = t("options.coming_soon");
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-2 text-center text-muted-foreground",
            p { class: "text-sm font-medium text-foreground", "{label}" }
            p { class: "text-xs", "{note}" }
        }
    }
}

#[component]
fn Field(label: String, children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-1.5",
            span { class: "text-xs font-medium text-muted-foreground", "{label}" }
            {children}
        }
    }
}

#[component]
fn Toggle(label: String, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex cursor-pointer items-center gap-2 text-sm",
            input {
                r#type: "checkbox",
                class: "size-4 rounded border-input accent-[var(--color-primary)]",
                checked,
                onchange: move |event: FormEvent| onchange.call(event.checked()),
            }
            "{label}"
        }
    }
}

#[component]
fn NumberInput(value: i64, min: i64, max: i64, onchange: EventHandler<i64>) -> Element {
    rsx! {
        input {
            r#type: "number",
            class: "h-9 w-28 rounded-md border border-input bg-transparent px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            value: "{value}",
            min: "{min}",
            max: "{max}",
            onchange: move |event: FormEvent| {
                if let Ok(parsed) = event.value().parse::<i64>() {
                    onchange.call(parsed);
                }
            },
        }
    }
}

#[component]
fn SchedulerTab() -> Element {
    let t = use_t();
    let settings = use_settings();
    let current = settings.get();
    let start = current.schedule_start.clone();
    let stop = current.schedule_stop.clone();
    let enabled = current.schedule_enabled;

    rsx! {
        div { class: "flex flex-col gap-5",
            Toggle {
                label: t("options.scheduler.enabled"),
                checked: enabled,
                onchange: move |value: bool| settings.update(|s| s.schedule_enabled = value),
            }
            div { class: "flex gap-6",
                Field { label: t("options.scheduler.start"),
                    TimeInput { value: start, onchange: move |value: String| settings.update(|s| s.schedule_start = value) }
                }
                Field { label: t("options.scheduler.stop"),
                    TimeInput { value: stop, onchange: move |value: String| settings.update(|s| s.schedule_stop = value) }
                }
            }
            p { class: "text-xs text-muted-foreground", "{t(\"options.scheduler.hint\")}" }
        }
    }
}

#[component]
fn TimeInput(value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        input {
            r#type: "time",
            class: "h-9 rounded-md border border-input bg-transparent px-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            value,
            onchange: move |event: FormEvent| onchange.call(event.value()),
        }
    }
}
