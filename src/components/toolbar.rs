use dioxus::prelude::*;

use crate::components::icons::{Icon, IconName};
use crate::state::Downloads;
use crate::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::ui::input::Input;

#[component]
pub fn Toolbar(state: Downloads) -> Element {
    let selected = state.selected_ids();
    let has_selection = !selected.is_empty();
    let resume_ids = selected.clone();
    let pause_ids = selected.clone();
    let remove_ids = selected;
    let query_value = state.query.read().clone();

    rsx! {
        header { class: "flex items-center gap-2 border-b border-border bg-card px-3 py-2",
            Button {
                size: ButtonSize::Sm,
                onclick: move |_| state.open_add(),
                Icon { name: IconName::Plus }
                "New"
            }
            Button {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in resume_ids.clone() { state.resume(id); } },
                Icon { name: IconName::Play }
                "Resume"
            }
            Button {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in pause_ids.clone() { state.pause(id); } },
                Icon { name: IconName::Pause }
                "Pause"
            }
            Button {
                variant: ButtonVariant::Destructive,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in remove_ids.clone() { state.remove(id); } },
                Icon { name: IconName::Trash }
                "Remove"
            }

            label { class: "ml-auto flex w-72 items-center gap-2 rounded-md border border-input bg-transparent px-2",
                Icon { name: IconName::Search, class: "text-muted-foreground" }
                Input {
                    class: "h-7 border-0 px-0 shadow-none focus-visible:ring-0",
                    placeholder: "Search downloads…",
                    value: query_value,
                    oninput: move |value| state.set_query(value),
                }
            }
        }
    }
}
