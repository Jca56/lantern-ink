//! What Lantern's desktop says that a drawing is shown by: the font
//! `sans-serif` stands for (ARCHITECTURE §5.5). Read once, by whichever
//! front end is starting, before any text is set.

use std::path::Path;

/// The family Lantern's desktop is set in, where `lantern.toml` (in
/// `home`'s `.lantern/config`) names one (`[appearance] font_family`).
/// A generic name there names none: Lantern's default stands.
pub fn font(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(".lantern/config/lantern.toml")).ok()?;
    named(&text)
}

fn named(toml: &str) -> Option<String> {
    let config = lntrn_data::toml::parse(toml).ok()?;
    let family = config.path("appearance.font_family")?.as_str()?.trim();
    let generic = matches!(family.to_ascii_lowercase().as_str(), "" | "sans-serif" | "serif" | "monospace" | "system-ui");
    (!generic).then(|| family.to_owned())
}

/// Have text that asks for `sans-serif` set in the desktop's own font.
/// The family, when the desktop names one and it came in time (before
/// any text was set).
pub fn use_font(home: &Path) -> Option<String> {
    font(home).filter(|family| ink_doc::fonts::defaults(Some(family), None))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_generic_family_names_none() {
        assert_eq!(super::named("[appearance]\nfont_family = \"Lexend\"\n"), Some("Lexend".to_owned()));
        for none in ["[appearance]\nfont_family = \"sans-serif\"\n", "[appearance]\nfont_family = \"  \"\n", "[appearance]\nscale = 1.2\n", "not toml ["] {
            assert_eq!(super::named(none), None, "{none}");
        }
    }
}
