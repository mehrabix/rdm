//! System-tray entry (Ubuntu/GNOME shows these via the AppIndicator extension).
//!
//! Gives quick "pause all / resume all / quit" without focusing the window.

use dioxus::desktop::trayicon::menu::{Menu, MenuItem, PredefinedMenuItem};
use dioxus::desktop::trayicon::{self, DioxusTrayIcon};
use dioxus::desktop::{icon_from_memory, use_tray_menu_event_handler};
use dioxus::prelude::*;

use crate::state::Downloads;

const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/icon-32.png");

#[component]
pub fn Tray(state: Downloads) -> Element {
    use_hook(|| {
        // Some desktops have no StatusNotifier host; never take the app down
        // just because a tray couldn't be created.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let menu = Menu::new();
            let pause_all = MenuItem::with_id("pause_all", "Pause all", true, None);
            let resume_all = MenuItem::with_id("resume_all", "Resume all", true, None);
            let exit = MenuItem::with_id("exit", "Exit", true, None);
            let _ = menu.append_items(&[
                &pause_all,
                &resume_all,
                &PredefinedMenuItem::separator(),
                &exit,
            ]);
            let icon = icon_from_memory::<DioxusTrayIcon>(TRAY_ICON_PNG).ok();
            trayicon::init_tray_icon(menu, icon);
        }));
    });

    use_tray_menu_event_handler(move |event| match event.id().0.as_str() {
        "pause_all" => state.pause_all(),
        "resume_all" => state.resume_all(),
        "exit" => exit_app(),
        _ => {}
    });

    rsx! {}
}

/// Exit the process. Wrapped in a `()`-returning fn because a closure whose body
/// diverges infers a `!` return type, which the event handler rejects.
fn exit_app() {
    std::process::exit(0);
}
