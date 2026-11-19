//! The Mirai fork ships its own releases (`docs/release-channel.md`): update checks,
//! downloads and the releases link must point at the fork, never at upstream.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn updates_come_from_the_fork_repository() {
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.repo, "mirasins/mirai-reader-pdfcraft-fork");
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.id, "pdfcraft");
    // The update card names the fork's app and maintainer, never "PdfCraft by ArtCraft", and
    // the names installers register stay findable: this fork's MSI says "Mirai Reader", and
    // installs made before the rename said "PdfCraft" (or "PrintCraft", PdfCraft's old name).
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.name, "Mirai Reader");
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.developer, "mirasins");
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.former_names, &["PdfCraft", "PrintCraft"]);
}

#[test]
fn the_source_link_names_the_fork() {
    assert_eq!(pdfcraft_engine::links::GITHUB, "https://github.com/mirasins/mirai-reader-pdfcraft-fork");
}
