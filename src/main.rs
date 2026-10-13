mod app;
mod components;
mod i18n;
mod settings;
mod state;
mod ui;

#[cfg(feature = "desktop")]
fn main() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    // Log to stderr so failures are visible rather than silent.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Load preferences before anything reads them (the engine sizes itself from
    // these on first use).
    let boot = settings::boot().clone();

    let window = WindowBuilder::new()
        .with_title("RDM — Rust Download Manager")
        .with_inner_size(LogicalSize::new(1120.0, 720.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0))
        .with_always_on_top(false)
        .with_visible(!boot.start_minimized);

    // Window/taskbar icon, decoded at startup from the generated asset.
    let icon: dioxus::desktop::tao::window::Icon =
        dioxus::desktop::icon_from_memory(include_bytes!("../assets/icon-256.png"))
            .expect("decode app icon");

    // Dioxus installs a default native menu bar (Window / Edit / Help) via `muda`.
    // RDM's entire UI lives in the webview, so we drop it with `with_menu(None)`.
    // The compiled Tailwind CSS, injected into the document head *before* the
    // webview renders anything. This is what actually kills the unstyled flash:
    // `document::Style` is inserted by a hook *after* the first render, so the
    // webview paints the raw DOM first. `custom_head` is part of the HTML the
    // webview parses, so the styles are already applied at first paint.
    let head = format!(
        "<style>{}</style>",
        include_str!("../assets/tailwind.css")
    );

    let config = Config::new()
        .with_window(window)
        .with_icon(icon)
        .with_custom_head(head)
        // Paint the app's dark surface before the webview renders anything.
        .with_background_color((28, 26, 36, 255))
        // No right-click "Inspect Element" (and no devtools) in release builds.
        .with_disable_context_menu(!cfg!(debug_assertions))
        .with_menu(None);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app::App);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    dioxus::launch(app::App);
}
