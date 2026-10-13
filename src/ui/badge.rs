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
    Warning,
    Info,
}

#[component]
pub fn Badge(
    #[props(default)] variant: BadgeVariant,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let base = "inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide";

    let variant_cls = match variant {
        BadgeVariant::Default => "border-transparent bg-primary text-primary-foreground",
        BadgeVariant::Secondary => "border-transparent bg-secondary text-secondary-foreground",
        BadgeVariant::Outline => "border-border text-muted-foreground",
        BadgeVariant::Destructive => {
            "border-transparent bg-destructive/15 text-destructive"
        }
        BadgeVariant::Success => "border-transparent bg-success/15 text-success",
        BadgeVariant::Warning => "border-transparent bg-warning/20 text-warning",
        BadgeVariant::Info => "border-transparent bg-info/15 text-info",
    };

    let class = tw_merge!(base, variant_cls, class);

    rsx! {
        span { class, {children} }
    }
}
