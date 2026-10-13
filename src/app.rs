use dioxus::prelude::*;

use crate::components::{AddDialog, Sidebar, StatusBar, Toolbar, TransferTable, Tray};
use crate::state::use_downloads;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[component]
pub fn App() -> Element {
    let state = use_downloads();
    let add_open = state.show_add;

    rsx! {
        // `dx` compiles tailwind.css -> assets/tailwind.css; link it here.
        document::Stylesheet { href: TAILWIND_CSS }

        div { class: "flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground antialiased",
            // Invisible: installs the system-tray icon + its menu.
            Tray { state }
            Toolbar { state }
            div { class: "flex min-h-0 flex-1",
                Sidebar { state }
                TransferTable { state }
            }
            StatusBar { state }
            if add_open() {
                AddDialog { state }
            }
        }
    }
}
