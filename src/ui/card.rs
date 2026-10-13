use dioxus::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn Card(#[props(default)] class: String, children: Element) -> Element {
    let class = tw_merge!("rounded-lg border border-border bg-card text-card-foreground", class);
    rsx! { div { class, {children} } }
}

#[component]
pub fn CardHeader(#[props(default)] class: String, children: Element) -> Element {
    let class = tw_merge!("flex flex-col gap-1 p-4", class);
    rsx! { div { class, {children} } }
}

#[component]
pub fn CardTitle(#[props(default)] class: String, children: Element) -> Element {
    let class = tw_merge!("text-sm font-semibold leading-none tracking-tight", class);
    rsx! { h3 { class, {children} } }
}

#[component]
pub fn CardDescription(#[props(default)] class: String, children: Element) -> Element {
    let class = tw_merge!("text-xs text-muted-foreground", class);
    rsx! { p { class, {children} } }
}

#[component]
pub fn CardContent(#[props(default)] class: String, children: Element) -> Element {
    let class = tw_merge!("p-4 pt-0", class);
    rsx! { div { class, {children} } }
}
