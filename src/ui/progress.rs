use dioxus::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn Progress(
    #[props(default = 0.0)] value: f64,
    #[props(default)] indeterminate: bool,
    #[props(default)] class: String,
) -> Element {
    // An indeterminate transfer (unknown Content-Length) shows a pulsing bar.
    let shift = if indeterminate {
        0.0
    } else {
        100.0 - value.clamp(0.0, 1.0) * 100.0
    };

    let class = tw_merge!(
        "relative h-1.5 w-full overflow-hidden rounded-full bg-secondary",
        class
    );

    rsx! {
        div { class,
            div {
                class: if indeterminate {
                    "h-full w-1/3 animate-pulse rounded-full bg-primary"
                } else {
                    "h-full w-full rounded-full bg-primary transition-transform duration-300"
                },
                style: if indeterminate { String::new() } else { format!("transform: translateX(-{shift}%)") },
            }
        }
    }
}
