use dioxus::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn Separator(
    #[props(default)] vertical: bool,
    #[props(default)] class: String,
) -> Element {
    let class = if vertical {
        tw_merge!("mx-1 h-5 w-px shrink-0 bg-border", class)
    } else {
        tw_merge!("my-1 h-px w-full shrink-0 bg-border", class)
    };

    rsx! { div { class } }
}
