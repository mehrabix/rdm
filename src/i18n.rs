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

/// `(code, endonym)` for every language we ship a catalog for. [`LOCALES`] must
/// contain all of these; the picker lists exactly this set, so every choice
/// changes the UI. A saved code that isn't here is reset to English.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("zh", "中文 (简体)"),
    ("fa", "فارسی"),
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
    ("fa", include_str!("../locales/fa.json")),
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

/// Whether `code` ships a catalog. Used to reset a stale saved language.
pub fn is_shipped(code: &str) -> bool {
    LOCALES.iter().any(|(locale, _)| *locale == code)
}

/// Pure catalog lookup: `code`'s text for `key`, falling back to English and
/// finally to the key itself, so a missing translation is never a crash or an
/// empty label.
fn translate(code: &str, key: &str) -> String {
    let catalogs = catalogs();
    catalogs
        .get(code)
        .and_then(|catalog| catalog.get(key))
        .or_else(|| catalogs.get(FALLBACK).and_then(|catalog| catalog.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_string())
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

    /// Translate `key` for the currently selected language.
    pub fn t(&self, key: &str) -> String {
        translate(&self.code(), key)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_parses_and_has_english_keys() {
        let catalogs = catalogs();
        let english = catalogs.get("en").expect("en catalog");
        assert!(!english.is_empty(), "en catalog is empty");
        for (code, _) in LOCALES.iter().copied() {
            let catalog = catalogs
                .get(code)
                .unwrap_or_else(|| panic!("{code} catalog missing"));
            let mut have: Vec<&String> = catalog.keys().collect();
            let mut want: Vec<&String> = english.keys().collect();
            have.sort();
            want.sort();
            assert_eq!(have, want, "key set mismatch for locale `{code}`");
        }
    }

    #[test]
    fn translated_locales_override_english() {
        assert_ne!(translate("de", "menu.file"), translate("en", "menu.file"));
        assert_ne!(translate("zh", "toolbar.pause"), translate("en", "toolbar.pause"));
        assert_ne!(translate("ru", "table.speed"), translate("en", "table.speed"));
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        assert_eq!(translate("zz", "menu.file"), translate("en", "menu.file"));
    }

    #[test]
    fn missing_key_falls_back_to_the_key() {
        assert_eq!(translate("en", "no.such.key"), "no.such.key");
        assert_eq!(translate("de", "no.such.key"), "no.such.key");
    }

    #[test]
    fn every_supported_language_has_a_catalog() {
        let catalogs = catalogs();
        assert_eq!(LANGUAGES.len(), LOCALES.len());
        for (code, _) in LANGUAGES.iter().copied() {
            assert!(is_shipped(code), "`{code}` is offered but has no catalog");
            assert!(catalogs.contains_key(code));
        }
    }

    #[test]
    fn rtl_membership() {
        for code in ["ar", "fa", "he"] {
            assert!(RTL.contains(&code), "{code} should be right-to-left");
        }
        assert!(!RTL.contains(&"en"));
        assert!(!RTL.contains(&"de"));
    }

    #[test]
    fn shipped_languages_are_the_registered_catalogs() {
        for code in ["en", "es", "fr", "de", "pt", "ru", "zh", "fa"] {
            assert!(is_shipped(code), "`{code}` should ship a catalog");
        }
        assert!(!is_shipped("ar"), "`ar` has no catalog yet");
        assert!(!is_shipped("zz"));
    }
}
