//! Inline Lucide-style SVG icons.
//!
//! Self-contained on purpose: these are stroke icons rendered straight from
//! path data, so the UI has no icon-font or icon-crate dependency. Swap them
//! for the rust-ui `icons` crate when you pull components from the registry.

use dioxus::prelude::*;
use tw_merge::tw_merge;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum IconName {
    #[default]
    Download,
    Plus,
    Search,
    Play,
    Pause,
    Stop,
    Trash,
    Rotate,
    Folder,
    Archive,
    FileText,
    Music,
    Package,
    Video,
    Image,
    File,
    Check,
}

impl IconName {
    fn path(self) -> &'static str {
        use IconName::*;
        match self {
            Download => "M12 3v12m0 0 4-4m-4 4-4-4M4 21h16",
            Plus => "M5 12h14M12 5v14",
            Search => "M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16Zm10 18-4.35-4.35",
            Play => "M6 3l14 9-14 9z",
            Pause => "M6 4h4v16H6zM14 4h4v16h-4z",
            Stop => "M6 6h12v12H6z",
            Trash => "M3 6h18M8 6V4h8v2M6 6l1 15h10l1-15",
            Rotate => "M21 12a9 9 0 1 1-3-6.7M21 3v6h-6",
            Folder => "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
            Archive => "M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v2H3zM5 9v9a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9M10 13h4",
            FileText => "M6 3h8l4 4v14H6zM14 3v4h4M9 13h6M9 17h6",
            Music => "M9 18V6l10-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm10-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
            Package => "M12 3 3 7.5v9L12 21l9-4.5v-9zM3 7.5 12 12l9-4.5M12 12v9",
            Video => "M3 6a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM16 10l5-3v10l-5-3",
            Image => "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm13 8-5-5L5 21",
            File => "M6 3h8l4 4v14H6zM14 3v4h4",
            Check => "M20 6 9 17l-5-5",
        }
    }
}

#[component]
pub fn Icon(#[props(default)] name: IconName, #[props(default)] class: String) -> Element {
    let class = tw_merge!("size-4 shrink-0", class);
    rsx! {
        svg {
            class,
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.8",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: name.path() }
        }
    }
}
