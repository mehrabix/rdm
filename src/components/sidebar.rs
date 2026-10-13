use dioxus::prelude::*;
use tw_merge::tw_merge;

use crate::components::icons::{Icon, IconName};
use crate::i18n::Tr;
use crate::state::{Downloads, Filter};

#[component]
pub fn Sidebar(state: Downloads) -> Element {
    let queues = crate::settings::use_settings().get().queue_names();

    rsx! {
        aside { class: "flex w-56 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-border bg-card/40 p-2",
            for filter in Filter::ALL {
                SidebarItem { state, filter }
            }

            div { class: "mt-3 flex flex-col gap-0.5",
                span { class: "px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                    Tr { k: "sidebar.queues" }
                }
                for name in queues {
                    QueueRow { key: "{name}", state, name }
                }
            }
        }
    }
}

#[component]
fn QueueRow(state: Downloads, name: String) -> Element {
    let mut stopped = use_signal(|| false);
    let is_stopped = stopped();
    let count = state
        .tasks
        .read()
        .iter()
        .filter(|task| task.spec.queue_name() == name.as_str())
        .count();
    let toggle_name = name.clone();

    rsx! {
        div { class: "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-muted-foreground hover:bg-muted",
            Icon { name: IconName::Package, class: "size-3.5 text-primary/70" }
            span { class: "truncate", "{name}" }
            span { class: "ms-auto text-xs tabular-nums opacity-70", "{count}" }
            button {
                class: "rounded px-1 text-[10px] font-medium uppercase tracking-wide text-primary hover:bg-primary/15",
                onclick: move |_| {
                    let now_stopped = !stopped();
                    stopped.set(now_stopped);
                    if now_stopped {
                        state.stop_queue(toggle_name.clone());
                    } else {
                        state.start_queue(toggle_name.clone());
                    }
                },
                Tr { k: if is_stopped { "queue.start" } else { "queue.stop" } }
            }
        }
    }
}

#[component]
fn SidebarItem(state: Downloads, filter: Filter) -> Element {
    let active = *state.filter.read() == filter;
    let count = state.count(filter);

    let class = tw_merge!(
        "flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm transition-colors",
        if active {
            "bg-primary/15 font-medium text-foreground"
        } else {
            "text-muted-foreground hover:bg-muted hover:text-foreground"
        }
    );
    let icon_class = tw_merge!(
        "size-3.5",
        if active { "text-primary" } else { "text-muted-foreground/70" }
    );
    let count_class = tw_merge!(
        "ms-auto rounded-full px-1.5 text-[11px] tabular-nums",
        if active {
            "bg-primary/20 text-primary"
        } else {
            "text-muted-foreground"
        }
    );

    rsx! {
        button {
            class,
            onclick: move |_| state.set_filter(filter),
            Icon { name: icon_for(filter), class: icon_class }
            span { class: "truncate", Tr { k: filter.label_key() } }
            span { class: count_class, "{count}" }
        }
    }
}

fn icon_for(filter: Filter) -> IconName {
    match filter {
        Filter::All => IconName::Download,
        Filter::Active => IconName::Play,
        Filter::Queued => IconName::Pause,
        Filter::Finished => IconName::Check,
        Filter::Unfinished => IconName::Rotate,
        Filter::Compressed => IconName::Archive,
        Filter::Documents => IconName::FileText,
        Filter::Music => IconName::Music,
        Filter::Programs => IconName::Package,
        Filter::Video => IconName::Video,
        Filter::Images => IconName::Image,
        Filter::Other => IconName::File,
    }
}
