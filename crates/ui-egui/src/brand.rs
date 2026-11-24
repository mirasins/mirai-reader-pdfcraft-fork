//! The Mirai fork's display name for the app.
//!
//! Upstream calls the app PdfCraft, in its own strings and in every translation in craft-i18n.
//! The fork shows it as "Mirai Reader" without editing those catalogs: [`rebrand`] swaps the name in
//! every translated interface string ([`crate::i18n::tr`] and its context variants), and the few
//! places that draw the name without translating it (window titles, the title bar) use
//! [`APP_NAME`]. Identifiers stay upstream's: the binary, crate names, the app id, the settings
//! folder and the installer's registry keys, so settings and installs carry over.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The name the fork shows for the app.
pub const APP_NAME: &str = "Mirai Reader";

/// The fork's icon, also used for compact UI marks.
pub(crate) const MARK: &[u8] = include_bytes!("../../../assets/app-icon/pdfcraft-small.svg");

/// The upstream credit, shown in the About dialog and the README: the original project, by name.
/// Drawn as it stands, never through [`crate::i18n::tr`]: [`rebrand`] renames the fork's own
/// interface strings, and this must keep the original project's name (a test pins it).
pub const UPSTREAM_CREDIT: &str = "Based on PdfCraft by Storytold, part of ArtCraft.";

/// What the credit must also say: the fork stands on its own. Shown beside
/// [`UPSTREAM_CREDIT`], also drawn as it stands.
pub const FORK_NOTICE: &str = "Mirai is an independent fork, not affiliated with ArtCraft or Storytold.";

/// Upstream's name for the app, as its interface strings spell it.
const UPSTREAM_NAME: &str = "PdfCraft";

/// Distinct rebranded strings kept. Interface strings are a fixed set (catalog rows and literals),
/// so this is never reached in practice; it guards against unbounded growth all the same: past it,
/// a string not seen before is shown with upstream's name rather than kept.
const CACHE_LIMIT: usize = 8192;

/// `s` with the app's upstream name replaced by [`APP_NAME`]; `s` itself when it doesn't name the
/// app. The rebranded copies are cached for the life of the process, so a string is rebuilt once.
pub fn rebrand(s: &str) -> &str {
    if !s.contains(UPSTREAM_NAME) {
        return s;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    // A poisoned lock (a panic elsewhere while holding it) only costs the rename, never a crash.
    let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() else { return s };
    if let Some(&branded) = cache.get(s) {
        return branded;
    }
    if cache.len() >= CACHE_LIMIT {
        return s;
    }
    let branded: &'static str = Box::leak(s.replace(UPSTREAM_NAME, APP_NAME).into_boxed_str());
    cache.insert(s.to_owned(), branded);
    branded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_upstream_name_becomes_the_fork_name() {
        assert_eq!(rebrand("About PdfCraft"), "About Mirai Reader");
        assert_eq!(rebrand("PdfCraft"), APP_NAME);
        assert_eq!(rebrand("Acerca de PdfCraft"), "Acerca de Mirai Reader");
    }

    #[test]
    fn other_strings_are_left_alone() {
        let s = "Save As";
        assert!(std::ptr::eq(rebrand(s), s));
        // The environment variable keeps its name.
        assert_eq!(rebrand("PDFCRAFT_PORTABLE"), "PDFCRAFT_PORTABLE");
    }

    #[test]
    fn a_string_is_rebuilt_once() {
        assert!(std::ptr::eq(rebrand("Quit PdfCraft"), rebrand("Quit PdfCraft")));
    }

    #[test]
    fn the_upstream_credit_names_the_original_project() {
        // The credit is drawn as it stands, never through `i18n::tr`: `rebrand` would rename the
        // upstream project in it, as it does in every one of the fork's own interface strings. This
        // pins what the About dialog must show: the original names, never the fork's.
        assert!(UPSTREAM_CREDIT.contains(UPSTREAM_NAME));
        assert!(UPSTREAM_CREDIT.contains("Storytold"));
        assert!(UPSTREAM_CREDIT.contains("ArtCraft"));
        assert!(!UPSTREAM_CREDIT.contains(APP_NAME));
        assert!(!FORK_NOTICE.contains(APP_NAME));
    }
}

/// The Mirai Reader mark at `size`.
pub(crate) fn mark(ui: &mut egui::Ui, size: craft::ui::display::PictureSize) {
    use craft::ui::contract::Show as _;
    use craft::ui::display::Picture;

    let _ = Picture::svg(APP_NAME.to_string(), MARK).extent(size).show(ui);
}
