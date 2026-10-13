//! System-tray entry (Ubuntu/GNOME shows these via the AppIndicator extension).
//!
//! Right-click shows the active downloads (IDM-style), plus pause all / resume
//! all / exit, without focusing the window.

use dioxus::desktop::trayicon::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use dioxus::desktop::trayicon::{self, DioxusTrayIcon, use_tray_icon};
use dioxus::desktop::{icon_from_memory, use_tray_menu_event_handler};
use dioxus::prelude::*;
use rdm_core::DownloadTask;

use crate::state::{file_name, Downloads};

const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/icon-32.png");

#[component]
pub fn Tray(state: Downloads) -> Element {
    use_hook(|| {
        // Some desktops have no StatusNotifier host; never take the app down
        // just because a tray couldn't be created.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let icon = icon_from_memory::<DioxusTrayIcon>(TRAY_ICON_PNG).ok();
            trayicon::init_tray_icon(build_menu(&[]), icon);
        }));
    });

    let tray = use_tray_icon();
    // Rebuild the menu only when the *set* of downloads changes, not on every
    // progress tick, to avoid churn.
    let mut signature = use_signal(String::new);

    use_effect(move || {
        let sig = {
            let tasks = state.tasks.read();
            tasks
                .iter()
                .filter(|task| !task.status.is_terminal())
                .map(|task| format!("{}:{:?}", task.id, task.status))
                .collect::<Vec<_>>()
                .join(",")
        };
        if *signature.read() == sig {
            return;
        }
        signature.set(sig);
        if let Some(tray) = &tray {
            let menu = build_menu(&state.tasks.read());
            let _ = tray.set_menu(Some(Box::new(menu)));
        }
    });

    use_tray_menu_event_handler(move |event| match event.id().0.as_str() {
        "pause_all" => state.pause_all(),
        "resume_all" => state.resume_all(),
        "exit" => exit_app(),
        _ => {}
    });

    rsx! {}
}

/// Build the tray menu: active downloads, then the global actions.
fn build_menu(tasks: &[DownloadTask]) -> Menu {
    let menu = Menu::new();

    let pause_all = MenuItem::with_id("pause_all", "Pause all", true, None);
    let resume_all = MenuItem::with_id("resume_all", "Resume all", true, None);
    // The toolkit's own quit is delivered natively (our menu-event loop is not
    // reliable under libayatana-appindicator), so it is what actually exits.
    let exit = PredefinedMenuItem::quit(Some("Exit"));
    let header = MenuItem::with_id("active", "Active downloads", false, None);
    let none = MenuItem::with_id("none", "No active downloads", false, None);
    let sep_top = PredefinedMenuItem::separator();
    let sep_bottom = PredefinedMenuItem::separator();

    let active: Vec<MenuItem> = tasks
        .iter()
        .filter(|task| !task.status.is_terminal())
        .take(12)
        .map(|task| MenuItem::with_id(format!("task:{}", task.id), task_label(task), false, None))
        .collect();

    let mut items: Vec<&dyn IsMenuItem> = vec![&pause_all, &resume_all, &sep_top, &header];
    if active.is_empty() {
        items.push(&none);
    } else {
        items.extend(active.iter().map(|item| item as &dyn IsMenuItem));
    }
    items.push(&sep_bottom);
    items.push(&exit);

    let _ = menu.append_items(&items);
    menu
}

fn task_label(task: &DownloadTask) -> String {
    let name = file_name(task);
    let name = if name.chars().count() > 40 {
        let short: String = name.chars().take(40).collect();
        format!("{short}…")
    } else {
        name
    };
    match task.progress() {
        Some(progress) if task.status.is_active() => {
            format!("{name}  {}%", (progress * 100.0).round() as i32)
        }
        _ => format!("{name}  {:?}", task.status),
    }
}

/// Exit the process. Wrapped in a `()`-returning fn because a closure whose body
/// diverges infers a `!` return type, which the event handler rejects.
fn exit_app() {
    std::process::exit(0);
}
