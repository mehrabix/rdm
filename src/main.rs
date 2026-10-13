mod app;
mod components;
mod state;
mod ui;

#[cfg(feature = "desktop")]
fn main() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    let window = WindowBuilder::new()
        .with_title("RDM — Rust Download Manager")
        .with_inner_size(LogicalSize::new(1120.0, 720.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0))
        .with_always_on_top(false);

    // Window/taskbar icon, decoded at startup from the generated asset.
    let icon: dioxus::desktop::tao::window::Icon =
        dioxus::desktop::icon_from_memory(include_bytes!("../assets/icon-256.png"))
            .expect("decode app icon");

    // Dioxus installs a default native menu bar (Window / Edit / Help) via `muda`.
    // RDM's entire UI lives in the webview, so we drop it with `with_menu(None)`.
    let config = Config::new()
        .with_window(window)
        .with_icon(icon)
        .with_menu(None);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app::App);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    dioxus::launch(app::App);
}
