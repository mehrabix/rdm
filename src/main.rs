use dioxus::prelude::*;

mod app;
mod components;
mod state;
mod ui;

fn main() {
    dioxus::launch(app::App);
}
