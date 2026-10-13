use dioxus::prelude::*;
use rdm_core::format::human_speed;

use crate::state::Downloads;

#[component]
pub fn StatusBar(state: Downloads) -> Element {
    let active = state.active_count();
    let queued = state.queued_count();
    let speed_text = human_speed(state.total_speed());

    let dot = if active > 0 {
        "size-1.5 rounded-full bg-success animate-pulse"
    } else {
        "size-1.5 rounded-full bg-muted-foreground/40"
    };

    rsx! {
        footer { class: "flex items-center gap-4 border-t border-border bg-card px-4 py-1.5 text-xs text-muted-foreground",
            span { class: "inline-flex items-center gap-1.5",
                span { class: dot }
                "{active} active"
            }
            span { "{queued} queued" }
            span { class: "ml-auto inline-flex items-center gap-1 tabular-nums",
                "Total: "
                span { class: "font-medium text-primary", "{speed_text}" }
            }
            span { class: "rounded bg-muted px-1.5 py-0.5 font-medium text-foreground/70", "RDM 0.1.0" }
        }
    }
}
