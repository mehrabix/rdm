use dioxus::prelude::*;
use tw_merge::tw_merge;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Destructive,
    Success,
}

#[component]
pub fn Badge(
    #[props(default)] variant: BadgeVariant,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let base = "inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide";

    let variant_cls = match variant {
        BadgeVariant::Default => "border-transparent bg-primary text-primary-foreground",
        BadgeVariant::Secondary => "border-transparent bg-secondary text-secondary-foreground",
        BadgeVariant::Outline => "border-border text-foreground",
        BadgeVariant::Destructive => {
            "border-transparent bg-destructive text-destructive-foreground"
        }
        BadgeVariant::Success => "border-transparent bg-emerald-600 text-white",
    };

    let class = tw_merge!(base, variant_cls, class);

    rsx! {
        span { class, {children} }
    }
}
