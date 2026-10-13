use dioxus::prelude::*;
use tw_merge::tw_merge;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Default,
    Lg,
    Icon,
}

#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] class: String,
    #[props(default)] disabled: bool,
    #[props(default)] onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let base = "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0";

    let variant_cls = match variant {
        ButtonVariant::Default => "bg-primary text-primary-foreground hover:bg-primary/90",
        ButtonVariant::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ButtonVariant::Outline => {
            "border border-input bg-transparent hover:bg-accent hover:text-accent-foreground"
        }
        ButtonVariant::Ghost => "hover:bg-accent hover:text-accent-foreground",
        ButtonVariant::Destructive => {
            "bg-destructive text-destructive-foreground hover:bg-destructive/90"
        }
        ButtonVariant::Link => "text-primary underline-offset-4 hover:underline",
    };

    let size_cls = match size {
        ButtonSize::Sm => "h-8 rounded-md px-3 text-xs",
        ButtonSize::Default => "h-9 px-4 py-2",
        ButtonSize::Lg => "h-10 rounded-md px-8",
        ButtonSize::Icon => "size-9",
    };

    let class = tw_merge!(base, variant_cls, size_cls, class);

    rsx! {
        button {
            class,
            disabled,
            onclick: move |event| onclick.call(event),
            {children}
        }
    }
}
