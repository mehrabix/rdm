use dioxus::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn Input(
    #[props(default)] class: String,
    #[props(default)] placeholder: String,
    #[props(default)] value: String,
    #[props(default)] disabled: bool,
    #[props(default)] oninput: EventHandler<String>,
) -> Element {
    let class = tw_merge!(
        "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        class
    );

    rsx! {
        input {
            class,
            r#type: "text",
            placeholder,
            value,
            disabled,
            oninput: move |event| oninput.call(event.value()),
        }
    }
}
