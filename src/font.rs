//! The bundled UI font.
//!
//! RDM ships **Vazirmatn** (SIL Open Font License, see `assets/fonts/OFL.txt`)
//! so the interface reads the same in Latin and in the RTL locales — Persian
//! and Arabic script render in the same family as the rest of the UI.
//!
//! The variable face is inlined as a `data:` URL rather than referenced as a
//! file: the stylesheet is injected into the webview's HTML, and a `url(...)`
//! would depend on how assets are served in each packaging format. Embedding
//! keeps the font working identically in the AppImage, the `.dmg` and the
//! `.msi`. The static weights ship next to it in `assets/fonts/` for builds
//! that cannot use a variable font, and `Vazirmatn-Variable.ttf` for anyone who
//! wants the desktop font.

use std::fmt::Write;

/// The variable face: one file covering weights 100–900.
const VAZIRMATN: &[u8] = include_bytes!("../assets/fonts/Vazirmatn-Variable.woff2");

/// The stack Vazirmatn is prepended to. The trailing families cover a missing
/// font (Noto Naskh Arabic keeps Persian readable on a bare Linux desktop).
const FALLBACK_STACK: &str = r#""Vazirmatn", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", "Noto Naskh Arabic", sans-serif"#;

/// `@font-face` plus a base rule, ready to append to the injected stylesheet.
pub fn stylesheet() -> String {
    let mut css = String::with_capacity(VAZIRMATN.len() * 4 / 3 + 512);
    let _ = write!(
        css,
        r#"
@font-face {{
  font-family: "Vazirmatn";
  font-style: normal;
  font-weight: 100 900;
  font-display: swap;
  src: url(data:font/woff2;base64,{}) format("woff2");
}}

html, body, button, input, select, textarea {{
  font-family: {FALLBACK_STACK};
}}
"#,
        base64(VAZIRMATN)
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

    #[test]
    fn stylesheet_embeds_the_font_and_names_the_family() {
        let css = stylesheet();
        assert!(css.contains("font-family: \"Vazirmatn\""));
        assert!(css.contains("url(data:font/woff2;base64,"));
        assert!(css.contains("font-weight: 100 900"));
        // The embedded data must be the real font, not an empty payload.
        assert!(css.len() > VAZIRMATN.len());
    }
}
