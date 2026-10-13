use dioxus::prelude::*;

use crate::components::{
    AddDialog, BatchDialog, DetailPanel, MenuBar, OptionsDialog, Sidebar, StatusBar, Toolbar,
    TransferTable, Tray,
};
use crate::i18n::{provide_settings, use_i18n};
use crate::settings;
use crate::state::use_downloads;

/// Apply the theme by toggling `dark` on `<html>`. `system` follows the OS and
/// keeps following it: a `matchMedia` listener is installed once.
fn apply_theme(mode: &str) {
    let script = format!(
        r#"
        window.__rdmTheme = "{mode}";
        (function () {{
            var apply = function () {{
                var m = window.__rdmTheme || "system";
                var dark = m === "dark" ||
                    (m === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
                document.documentElement.classList.toggle("dark", dark);
            }};
            window.__rdmApplyTheme = apply;
            if (!window.__rdmThemeListener) {{
                window.__rdmThemeListener = true;
                window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", function () {{
                    if (window.__rdmApplyTheme) window.__rdmApplyTheme();
                }});
            }}
            apply();
        }})();
        "#
    );
    let _ = document::eval(&script);
}

#[component]
pub fn App() -> Element {
    // Settings first: the engine sizes itself from them on first use.
    let settings = use_signal(|| settings::boot().clone());
    provide_settings(settings);
    let i18n = use_i18n();

    let state = use_downloads();
    let add_open = state.show_add;
    let options_open = state.show_options;

    // Re-apply the theme and push the speed limit whenever a preference changes.
    use_effect(move || {
        let current = settings.read();
        apply_theme(current.theme.as_js());
        crate::state::engine().set_speed_limit(current.speed_limit_kbps as u64 * 1024);
    });

    // Scheduler: outside the daily window, pause everything; when it opens
    // again, resume only what the scheduler itself paused.
    use_future(move || async move {
        let mut paused_by_schedule: Vec<String> = Vec::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let current = settings.read().clone();
            if !current.schedule_enabled {
                paused_by_schedule.clear();
                continue;
            }
            if crate::settings::in_schedule(&current, crate::settings::now_minutes()) {
                for id in paused_by_schedule.drain(..) {
                    state.resume(id);
                }
            } else {
                for id in state.running_ids() {
                    state.pause(id.clone());
                    paused_by_schedule.push(id);
                }
            }
        }
    });

    let direction = if i18n.is_rtl() { "rtl" } else { "ltr" };

    rsx! {
        div {
            class: "flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground antialiased",
            dir: direction,

            // Invisible: installs the system-tray icon + its menu.
            Tray { state }
            MenuBar { state }
            Toolbar { state }

            div { class: "flex min-h-0 flex-1",
                Sidebar { state }
                TransferTable { state }
                DetailPanel { state }
            }

            StatusBar { state }

            if add_open() {
                AddDialog { state }
            }
            if *state.show_batch.read() {
                BatchDialog { state }
            }
            if options_open() {
                OptionsDialog { state }
            }
        }
    }
}
