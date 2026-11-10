//! The Mirai fork ships its own releases (`docs/release-channel.md`): update checks,
//! downloads and the releases link must point at the fork, never at upstream.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn updates_come_from_the_fork_repository() {
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.repo, "mirasins/mirai-reader-pdfcraft-fork");
    assert_eq!(pdfcraft_ui_egui::UPDATE_APP.id, "pdfcraft");
}

#[test]
fn the_source_link_names_the_fork() {
    assert_eq!(pdfcraft_engine::links::GITHUB, "https://github.com/mirasins/mirai-reader-pdfcraft-fork");
}
