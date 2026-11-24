//! Help ▸ About: the framework's hosted About dialog over the app's info and the baked credits.

use craft::behavior::IconId;
use craft::ui::patterns::form_dialogs::{AboutImage, AboutInfo, AboutLink, AboutTab, HostedAbout};

use crate::PdfCraftApp;

use crate::brand;

/// What the About tab shows: the Mirai identity, original-project credit and community links.
fn info() -> AboutInfo {
    let info = AboutInfo::new(brand::APP_NAME, brand::UPSTREAM_CREDIT, pdfcraft_engine::build_info::version())
        .mark(AboutImage::svg("Mirai", brand::MARK))
        .built_with(brand::FORK_NOTICE);
    use pdfcraft_engine::links;
    let upstream = [
        AboutLink::new("PdfCraft on GitHub (original project)", links::UPSTREAM).icon(IconId::new("code-xml")),
        AboutLink::new("ArtCraft Discord (original project)", links::DISCORD).icon(IconId::new("messages-square")),
    ];
    info.links(
        links::LINKS
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let label = if i == 0 { tl!("Join our community") } else { tl!(l.label) };
                AboutLink::new(label.to_string(), l.url).icon(IconId::new(l.icon))
            })
            .chain(upstream),
    )
}

impl PdfCraftApp {
    /// Opens the About window on `tab` (Help ▸ About, `help.about`).
    pub(crate) fn open_about(&mut self, tab: AboutTab) {
        crate::dialogs::host::open(self, HostedAbout::new(info(), crate::credits_data::credits()).tab(tab));
    }

    /// Opens the link the user activated in the About window, the way the `help.*` link commands do.
    pub(crate) fn about_pass(&mut self, _ctx: &egui::Context) {
        let Some(url) = self.dialogs.get_mut::<HostedAbout>().and_then(HostedAbout::take_link) else {
            return;
        };
        self.open_url(&url);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn about_community_links_keep_their_icons_and_destinations() {
        let info = super::info();
        assert_eq!(info.links.len(), pdfcraft_engine::links::LINKS.len() + 2);
        for (shown, expected) in info.links.iter().zip(pdfcraft_engine::links::LINKS) {
            assert_eq!(shown.url, expected.url);
            assert_eq!(shown.icon, Some(craft::behavior::IconId::new(expected.icon)));
        }
    }
}
