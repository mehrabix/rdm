//! Runtime language switching.
//!
//! Strings live in `locales/<code>.json` as a flat `key -> text` map, embedded
//! at compile time. Translations are authored once in `locales/translations.tsv`
//! and expanded by `scripts/gen-locales.py`, so adding a language is one column
//! plus one entry in [`LOCALES`].
//!
//! Any locale not in [`LOCALES`], and any key missing from a locale, falls back
//! to English.

use std::collections::HashMap;
use std::sync::OnceLock;

use dioxus::prelude::*;

use crate::settings::{Settings, SettingsCtx};

/// `(code, endonym)` — mirrors the languages IDM offers. A language appears here
/// (so it can be picked) even before its translations land; it then falls back
/// to English.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ar", "العربية"),
    ("az", "Azərbaycan dili"),
    ("bg", "Български"),
    ("bs", "Bosanski"),
    ("zh", "中文 (简体)"),
    ("hr", "Hrvatski"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("nl", "Nederlands"),
    ("fa", "فارسی"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("el", "Ελληνικά"),
    ("he", "עברית"),
    ("hu", "Magyar"),
    ("it", "Italiano"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("lt", "Lietuvių"),
    ("mk", "Македонски"),
    ("no", "Norsk"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ro", "Română"),
    ("ru", "Русский"),
    ("sr", "Српски"),
    ("sk", "Slovenčina"),
    ("sl", "Slovenščina"),
    ("es", "Español"),
    ("th", "ไทย"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("vi", "Tiếng Việt"),
];

/// Right-to-left scripts among the supported languages.
pub const RTL: &[&str] = &["ar", "fa", "he"];

/// Every locale that ships real translations. Others fall back to English.
const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.json")),
    ("es", include_str!("../locales/es.json")),
    ("fr", include_str!("../locales/fr.json")),
    ("de", include_str!("../locales/de.json")),
    ("pt", include_str!("../locales/pt.json")),
    ("ru", include_str!("../locales/ru.json")),
    ("zh", include_str!("../locales/zh.json")),
];

const FALLBACK: &str = "en";

fn catalogs() -> &'static HashMap<&'static str, HashMap<String, String>> {
    static CATALOGS: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        LOCALES
            .iter()
            .map(|(code, json)| (*code, load(json)))
            .collect()
    })
}

fn load(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// The translator, shared through context. `t()` reads the language from
/// settings, so a component that calls it re-renders when the language changes.
#[derive(Clone, Copy, PartialEq)]
pub struct I18n {
    pub settings: Signal<Settings>,
}

impl I18n {
    pub fn code(&self) -> String {
        self.settings.read().language.clone()
    }

    pub fn is_rtl(&self) -> bool {
        RTL.contains(&self.code().as_str())
    }

    /// Translate `key`, falling back to English and finally to the key itself,
    /// so a missing translation is never a crash or an empty label.
    pub fn t(&self, key: &str) -> String {
        let catalogs = catalogs();
        let code = self.code();
        catalogs
            .get(code.as_str())
            .and_then(|catalog| catalog.get(key))
            .or_else(|| catalogs.get(FALLBACK).and_then(|catalog| catalog.get(key)))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }
}

pub fn use_i18n() -> I18n {
    use_context::<I18n>()
}

/// A callable translator for a component: `let t = use_t(); let s = t("toolbar.new");`
pub fn use_t() -> impl Fn(&str) -> String + Copy {
    let i18n = use_i18n();
    move |key: &str| i18n.t(key)
}

/// Inline translation node: `Tr { k: "toolbar.new" }`.
///
/// Preferred over interpolating `t(..)` inside an rsx string, which the rsx
/// parser cannot handle.
#[component]
pub fn Tr(#[props(into)] k: String) -> Element {
    let i18n = use_i18n();
    let text = i18n.t(&k);
    rsx! {
        "{text}"
    }
}

/// Provide both the settings and the translator from one signal.
pub fn provide_settings(settings: Signal<Settings>) {
    use_context_provider(|| SettingsCtx { settings });
    use_context_provider(|| I18n { settings });
}
