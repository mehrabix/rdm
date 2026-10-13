//! A shadcn/rust-ui style Select: a styled trigger that opens a popover list.
//!
//! We deliberately avoid the native `<select>` element — it ignores our theme
//! and looks out of place inside the rest of the UI.

use dioxus::prelude::*;
use tw_merge::tw_merge;

use crate::components::icons::{Icon, IconName};

#[component]
pub fn Select(
    /// Currently selected value.
    #[props(default)] value: String,
    /// Shown when no option matches `value`.
    #[props(default)] placeholder: String,
    #[props(default)] class: String,
    #[props(default)] disabled: bool,
    #[props(default)] onchange: EventHandler<String>,
    /// `(value, label)` pairs.
    options: Vec<(String, String)>,
) -> Element {
    let mut open = use_signal(|| false);
    let is_open = open();
    let selected_label = options
        .iter()
        .find(|(option_value, _)| option_value == &value)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| placeholder.clone());

    let trigger_class = tw_merge!(
        "flex h-9 w-full items-center justify-between gap-2 rounded-md border border-input bg-transparent px-3 text-sm transition-colors hover:bg-accent/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        class
    );
    let chevron_class = if is_open {
        "size-3.5 shrink-0 rotate-180 text-muted-foreground transition-transform"
    } else {
        "size-3.5 shrink-0 text-muted-foreground transition-transform"
    };

    rsx! {
        div { class: "relative",
            button {
                class: trigger_class,
                disabled,
                onclick: move |_| open.toggle(),
                span { class: "truncate", "{selected_label}" }
                Icon { name: IconName::ChevronDown, class: chevron_class }
            }

            if is_open {
                // click-away layer
                div { class: "fixed inset-0 z-40", onclick: move |_| open.set(false) }

                div {
                    class: "absolute left-0 top-full z-50 mt-1 max-h-72 w-full min-w-40 overflow-y-auto rounded-md border border-border bg-card p-1 shadow-xl",
                    for (option_value, label) in options.iter() {
                        {
                            let option_value = option_value.clone();
                            let is_selected = option_value == value;
                            let row_class = if is_selected {
                                "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm bg-accent text-accent-foreground"
                            } else {
                                "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent/60"
                            };
                            rsx! {
                                button {
                                    class: row_class,
                                    onclick: move |_| {
                                        onchange.call(option_value.clone());
                                        open.set(false);
                                    },
                                    span { class: "flex-1 truncate", "{label}" }
                                    if is_selected {
                                        Icon { name: IconName::Check, class: "size-3.5 text-primary" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
