use dioxus::prelude::*;
use tw_merge::tw_merge;

/// A stateless modal shell. Render it conditionally on your own `Signal<bool>`
/// and put any content inside; clicking the backdrop closes via `onclose`.
#[component]
pub fn Dialog(
    #[props(default)] class: String,
    #[props(default)] onclose: EventHandler<()>,
    children: Element,
) -> Element {
    let class = tw_merge!(
        "w-full max-w-lg rounded-xl border border-border bg-card p-6 shadow-2xl",
        class
    );

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm",
            onclick: move |_| onclose.call(()),
            div {
                class,
                onclick: move |event| event.stop_propagation(),
                {children}
            }
        }
    }
}
