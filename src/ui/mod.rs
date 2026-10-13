//! rust-ui style primitives. Each component is a plain, copy-pasteable Dioxus
//! component that merges its classes with `tw_merge!`, mirroring the Rust/UI
//! registry convention (`crate::ui::button::Button`, ...).

pub mod badge;
pub mod button;
pub mod card;
pub mod dialog;
pub mod input;
pub mod progress;
pub mod select;
pub mod separator;
