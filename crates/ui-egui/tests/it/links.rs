//! Community links and upstream credit remain distinct across the home screen and About dialog.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::links;
use pdfcraft_ui_egui::PdfCraftApp;

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| {
        crate::support::install_runtime(&cc.egui_ctx);
        {
            let mut app = crate::control::new_app();
            app.set_option(&egui::Context::default(), "language", "en").unwrap();
            setup(&mut app);
            app
        }
    });
    h.run_steps(4);
    h
}

#[test]
fn community_button_in_the_top_bar_opens_the_mirai_community() {
    let mut h = harness(|_| {});
    h.get_by_label("Community").click();
    assert_eq!(crate::support::step_collecting_urls(&mut h, 4), [links::COMMUNITY]);
    assert_eq!(links::COMMUNITY, "https://github.com/mirasins");
}

#[test]
fn home_screen_links() {
    for (label, url) in [
        ("Join our community", links::COMMUNITY),
        ("Report an issue", links::ISSUES),
        ("Mirai Reader on GitHub", links::GITHUB),
        ("ArtCraft website (the original project)", links::WEBSITE),
    ] {
        let mut h = harness(|_| {});
        h.get_by_role_and_label(egui::accesskit::Role::Group, "Join the Mirai community").get_by_label(label).click();
        assert_eq!(crate::support::step_collecting_urls(&mut h, 4), [url], "{label}");
    }
}

#[test]
fn home_community_card_closes_for_this_run() {
    let mut h = harness(|_| {});
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Join the Mirai community");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Join the Mirai community").get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().community_card_closed);
    assert!(h.query_by_label("Join the Mirai community").is_none());
    assert!(h.query_by_label("Join our community").is_none());
    // Not remembered: a fresh app shows the card again.
    assert!(!crate::control::new_app().community_card_closed);
}

#[test]
fn about_dialog_shows_the_brand_and_links() {
    let mut h = harness(|app| app.set_option(&egui::Context::default(), "dialog", "about").unwrap());
    settled(&mut h, "Mirai Reader");
    assert!(h.query_all_by_label("Mirai").count() >= 1, "the mark's alt text");
    h.get_by_label(pdfcraft_ui_egui::brand::UPSTREAM_CREDIT);
    h.get_by_label(pdfcraft_ui_egui::brand::FORK_NOTICE);
    settled(&mut h, "Join our community").click();
    assert_eq!(crate::support::step_collecting_urls(&mut h, 4), [links::COMMUNITY]);
}

#[test]
fn about_dialog_upstream_credit_links_open_upstream() {
    for (label, url) in [("PdfCraft on GitHub (original project)", links::UPSTREAM), ("ArtCraft Discord (original project)", links::DISCORD)] {
        let mut h = harness(|app| app.set_option(&egui::Context::default(), "dialog", "about").unwrap());
        settled(&mut h, label).click();
        assert_eq!(crate::support::step_collecting_urls(&mut h, 4), [url], "{label}");
    }
}

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it. A table row is named by
/// its first cell, so a name may be the row's and its cell's: the first (outer) node is the one returned.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, PdfCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_all_by_label(label).next().map(|n| n.rect());
        if now.is_some() && now == last {
            return h.query_all_by_label(label).next().expect("the node was just found");
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

#[test]
fn about_dialog_has_contributors_and_models_tabs() {
    let mut h = harness(|app| app.set_option(&egui::Context::default(), "dialog", "about").unwrap());
    settled(&mut h, "Contributors").click();
    // The owner is always in the compiled-in credits (contributors/contributors.json), shown by username.
    settled(&mut h, "@echelon");
    settled(&mut h, "Table").click();
    settled(&mut h, "PRs");
    settled(&mut h, "Display name").click();
    settled(&mut h, "Brandon Thomas");
    settled(&mut h, "Models").click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Anthropic").count() >= 1);
}
