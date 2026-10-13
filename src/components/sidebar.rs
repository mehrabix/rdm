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
    let label = filter.label();

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
        "ml-auto rounded-full px-1.5 text-[11px] tabular-nums",
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
            span { class: "truncate", "{label}" }
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
