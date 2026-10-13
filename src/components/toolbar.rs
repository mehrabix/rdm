use dioxus::prelude::*;

use crate::components::icons::{Icon, IconName};
use crate::i18n::{use_t, Tr};
use crate::settings::{use_settings, Theme};
use crate::state::Downloads;
use crate::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::ui::input::Input;

#[component]
pub fn Toolbar(state: Downloads) -> Element {
    let t = use_t();
    let settings = use_settings();

    let selected = state.selected_ids();
    let has_selection = !selected.is_empty();
    let resume_ids = selected.clone();
    let pause_ids = selected.clone();
    let remove_ids = selected;
    let query_value = state.query.read().clone();
    let placeholder = t("toolbar.search");

    // The theme button cycles System -> Light -> Dark and shows the current mode.
    let theme = settings.get().theme;
    let next_theme = match theme {
        Theme::System => Theme::Light,
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::System,
    };
    let theme_icon = match theme {
        Theme::System => IconName::Monitor,
        Theme::Light => IconName::Sun,
        Theme::Dark => IconName::Moon,
    };

    rsx! {
        header { class: "flex items-center gap-2 border-b border-border bg-card px-3 py-2",
            Button {
                size: ButtonSize::Sm,
                onclick: move |_| state.open_add(),
                Icon { name: IconName::Plus }
                Tr { k: "toolbar.new" }
            }
            Button {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in resume_ids.clone() { state.resume(id); } },
                Icon { name: IconName::Play }
                Tr { k: "toolbar.resume" }
            }
            Button {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in pause_ids.clone() { state.pause(id); } },
                Icon { name: IconName::Pause }
                Tr { k: "toolbar.pause" }
            }
            Button {
                variant: ButtonVariant::Destructive,
                size: ButtonSize::Sm,
                disabled: !has_selection,
                onclick: move |_| { for id in remove_ids.clone() { state.remove(id); } },
                Icon { name: IconName::Trash }
                Tr { k: "toolbar.remove" }
            }

            div { class: "ms-auto flex items-center gap-2",
                label { class: "flex w-64 items-center gap-2 rounded-md border border-input bg-transparent px-2",
                    Icon { name: IconName::Search, class: "text-muted-foreground" }
                    Input {
                        class: "h-7 border-0 px-0 shadow-none focus-visible:ring-0",
                        placeholder,
                        value: query_value,
                        oninput: move |value| state.set_query(value),
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    onclick: move |_| settings.update(|s| s.theme = next_theme),
                    Icon { name: theme_icon, class: "text-muted-foreground" }
                }
            }
        }
    }
}
