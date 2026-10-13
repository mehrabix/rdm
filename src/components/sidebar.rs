use dioxus::prelude::*;
use tw_merge::tw_merge;

use crate::components::icons::{Icon, IconName};
use crate::state::{Downloads, Filter};

#[component]
pub fn Sidebar(state: Downloads) -> Element {
    rsx! {
        aside { class: "flex w-56 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-border bg-card/40 p-2",
            for filter in Filter::ALL {
                SidebarItem { state, filter }
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
            "bg-accent font-medium text-accent-foreground"
        } else {
            "text-muted-foreground hover:bg-accent/60 hover:text-foreground"
        }
    );

    rsx! {
        button {
            class,
            onclick: move |_| state.set_filter(filter),
            Icon { name: icon_for(filter), class: "size-3.5" }
            span { class: "truncate", "{filter.label()}" }
            span { class: "ml-auto text-xs tabular-nums opacity-70", "{count}" }
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
