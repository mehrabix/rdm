//! The bundled UI font.
//!
//! RDM ships **Vazirmatn** (SIL Open Font License, see `assets/fonts/OFL.txt`)
//! so the interface reads the same in Latin and in the RTL locales — Persian and
//! Arabic script render in the same family as the rest of the UI.
//!
//! Four **static** weights are embedded rather than the variable face. A
//! variable font only varies if the engine applies its `wght` axis; when that is
//! ignored — which is what made the Latin text look uniformly thin — every label
//! paints at a single weight. One `@font-face` per weight is what the interface
//! actually asks for (`font-medium`, `font-semibold`, bold headings) and is what
//! every engine honours.
//!
//! The faces are inlined as `data:` URLs instead of referenced files: the
//! stylesheet is injected into the webview's HTML, and a `url(...)` would depend
//! on how assets are served in each packaging format. Embedding keeps the font
//! working identically in the AppImage, the `.dmg` and the `.msi`. The remaining
//! weights stay in `assets/fonts/` for other builds.

use std::fmt::Write;

/// `(weight, file)` for every face the interface uses: body text, `font-medium`,
/// `font-semibold`, and bold headings.
const FACES: &[(u16, &[u8])] = &[
    (400, include_bytes!("../assets/fonts/Vazirmatn-Regular.woff2")),
    (500, include_bytes!("../assets/fonts/Vazirmatn-Medium.woff2")),
    (600, include_bytes!("../assets/fonts/Vazirmatn-SemiBold.woff2")),
    (700, include_bytes!("../assets/fonts/Vazirmatn-Bold.woff2")),
];

/// The stack Vazirmatn is prepended to. The trailing families cover a missing
/// font (Noto Naskh Arabic keeps Persian readable on a bare Linux desktop).
const FALLBACK_STACK: &str = r#""Vazirmatn", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", "Noto Naskh Arabic", sans-serif"#;

/// The `@font-face` rules plus a base rule, ready to append to the injected
/// stylesheet.
pub fn stylesheet() -> String {
    let mut css = String::with_capacity(400_000);
    for (weight, bytes) in FACES {
        let _ = write!(
            css,
            "\n@font-face {{\n  font-family: \"Vazirmatn\";\n  font-style: normal;\n  font-weight: {weight};\n  font-display: swap;\n  src: url(data:font/woff2;base64,{}) format(\"woff2\");\n}}\n",
            base64(bytes)
        );
    }
    let _ = write!(
        css,
        "\nhtml, body, button, input, select, textarea {{\n  font-family: {FALLBACK_STACK};\n}}\n"
    );
    css
}

/// Standard base64 (RFC 4648). Hand-rolled to keep the app dependency-free.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xff, 0xff]), "////");
    }

    /// Every weight the UI asks for must be a declared face: a missing one is
    /// silently synthesised (or dropped), which is what made the text look off.
    #[test]
    fn one_face_per_weight_the_ui_uses() {
        let css = stylesheet();
        for weight in [400, 500, 600, 700] {
            assert!(
                css.contains(&format!("font-weight: {weight};")),
                "missing a @font-face for weight {weight}"
            );
        }
        assert_eq!(css.matches("@font-face").count(), 4, "one rule per weight");

        // No variable range: that is the form engines may ignore.
        assert!(!css.contains("100 900"), "the variable range must be gone");

        // The embedded data must be the real fonts, not empty payloads.
        assert_eq!(css.matches("url(data:font/woff2;base64,").count(), 4);
        let fonts: usize = FACES.iter().map(|(_, bytes)| bytes.len()).sum();
        assert!(css.len() > fonts, "the payload is smaller than the fonts");
    }

    #[test]
    fn the_family_and_fallbacks_are_applied_globally() {
        let css = stylesheet();
        assert!(css.contains("font-family: \"Vazirmatn\""));
        assert!(css.contains("html, body, button, input, select, textarea"));
        assert!(css.contains("\"Noto Naskh Arabic\""), "Arabic-script fallback");
    }
}
