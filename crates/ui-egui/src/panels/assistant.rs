//! Mirai Assistant: the side panel for summarizing, translating and asking about the open document
//! through the user's own Claude Code sign-in (`pdfcraft-ai`).
//!
//! Nothing is sent until the user presses a button. Both the sign-in check and the answer block
//! (up to 20 s and 5 min), so each runs on its own thread and reports through a channel that the
//! panel polls on every frame.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use pdfcraft_ai::{Request, Status, Task};
use pdfcraft_engine::DocId;
use pdfcraft_engine::export::{ExportSource, Exporter};

use super::{LeftPanel, PdfCraftApp, Tokens, ToolGroup, icons, widgets};
use crate::theme::ThemeKind;

/// "All pages" is refused above this many pages rather than silently reading a subset.
const MAX_PAGES: usize = 500;
/// The wording shown when a worker thread ends without an answer.
const STOPPED: &str = "The assistant stopped unexpectedly. Try again.";
/// The conversation keeps at most this many entries...
const MAX_HISTORY: usize = 8;
/// ...and at most this many bytes of text (the newest entry is always kept).
const MAX_HISTORY_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Current,
    All,
}

/// What the user is asking for; chosen with the selector above the input.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ask,
    Summarize,
    Translate,
    Explain,
}

/// A quick prompt under the composer. Clicking one only fills the question box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Chip {
    KeyPoints,
    Dates,
    Simply,
    People,
}

const CHIPS: [Chip; 4] = [Chip::KeyPoints, Chip::Dates, Chip::Simply, Chip::People];

/// The label and the question of a chip (English; translated where drawn).
fn chip_label(chip: Chip) -> &'static str {
    match chip {
        Chip::KeyPoints => "Key points",
        Chip::Dates => "Dates and deadlines",
        Chip::Simply => "Explain simply",
        Chip::People => "Who is involved?",
    }
}

fn chip_question(chip: Chip) -> &'static str {
    match chip {
        Chip::KeyPoints => "What are the key points of this document?",
        Chip::Dates => "List the dates and deadlines mentioned in this document.",
        Chip::Simply => "Explain this document in simple words.",
        Chip::People => "Who is involved, and what is each person's or organization's role?",
    }
}

/// Whether Claude Code has been asked about its sign-in.
enum StatusSlot {
    Unchecked,
    Checking(Receiver<Status>),
    Known(Status),
}

/// A request in flight.
struct Run {
    rx: Receiver<Result<String, String>>,
    cancel: Arc<AtomicBool>,
    pages: usize,
    heading: String,
    doc: DocId,
    started: web_time::Instant,
}

/// One answer (or why there is none), with what it answered.
struct Answer {
    heading: String,
    doc: DocId,
    body: Result<String, String>,
    /// The `[p. N]` citations of an `Ok` body, found once when the answer arrives rather than on every frame.
    cites: Vec<(std::ops::Range<usize>, u32)>,
}

/// The characters of a page's wrapper in the AI prompt (`<page n="">`, two newlines, `</page>`, a
/// newline) without the page number. All ASCII, so bytes equal characters.
const PAGE_WRAPPER_CHARS: usize = "<page n=\"\">\n\n</page>\n".len();

impl Answer {
    fn new(heading: String, doc: DocId, body: Result<String, String>) -> Self {
        let cites = match &body {
            Ok(text) => citations(text),
            Err(_) => Vec::new(),
        };
        Self { heading, doc, body, cites }
    }

    fn bytes(&self) -> usize {
        let body = match &self.body {
            Ok(s) | Err(s) => s.len(),
        };
        self.heading.len().saturating_add(body)
    }
}

/// Add `a` as the newest entry, dropping the oldest while there are more than [`MAX_HISTORY`]
/// entries or more than [`MAX_HISTORY_BYTES`] of text. The newest entry is always kept.
fn push_answer(history: &mut VecDeque<Answer>, a: Answer) {
    history.push_back(a);
    let mut bytes: usize = history.iter().fold(0usize, |n, e| n.saturating_add(e.bytes()));
    while history.len() > 1 && (history.len() > MAX_HISTORY || bytes > MAX_HISTORY_BYTES) {
        match history.pop_front() {
            Some(old) => bytes = bytes.saturating_sub(old.bytes()),
            None => break,
        }
    }
}

/// "8 s", "59 s", "1 min 05 s".
fn elapsed_label(secs: u64) -> String {
    if secs < 60 { format!("{secs} s") } else { format!("{} min {:02} s", secs / 60, secs % 60) }
}

/// The panel's state; it lives in the app so the conversation survives closing the panel.
pub struct AssistantState {
    status: StatusSlot,
    scope: Scope,
    mode: Mode,
    language: String,
    question: String,
    /// Set by a chip: focus the question box on the next frame.
    focus_question: bool,
    run: Option<Run>,
    /// Oldest first, newest last.
    history: VecDeque<Answer>,
}

impl Default for AssistantState {
    fn default() -> Self {
        Self {
            status: StatusSlot::Unchecked,
            scope: Scope::Current,
            mode: Mode::Ask,
            language: String::new(),
            question: String::new(),
            focus_question: false,
            run: None,
            history: VecDeque::new(),
        }
    }
}

/// A chip click: put its question in the box and switch to Ask. It never starts anything.
fn fill_question(st: &mut AssistantState, text: &str) {
    st.question = text.to_owned();
    st.mode = Mode::Ask;
    st.focus_question = true;
}

pub(super) fn assistant_panel(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens, g: &'static ToolGroup) {
    let (back, close) = super::panel_header(ui, t, g.label, Some(("chevron-left", tl!("Back to all tools"))));
    if back {
        app.left = LeftPanel::AllTools;
        app.enter_mode(crate::Mode::AllTools);
    }
    if close {
        app.left_open = false;
    }
    // The state is taken out so the panel can use `app` freely, and put back at the end.
    let mut st = std::mem::take(&mut app.assistant);
    if st.language.is_empty() {
        st.language = crate::i18n::current().english_name().to_owned();
    }
    poll(&mut st, ui.ctx());
    let go = draw(app, &mut st, ui, t);
    app.assistant = st;
    if let Some(page) = go
        && let Some(view) = app.active.and_then(|i| app.views.get_mut(i))
    {
        view.go_to_page(page);
    }
}

/// Collect finished work. Never blocks.
fn poll(st: &mut AssistantState, ctx: &egui::Context) {
    if let StatusSlot::Checking(rx) = &st.status {
        match rx.try_recv() {
            Ok(s) => st.status = StatusSlot::Known(s),
            Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => st.status = StatusSlot::Known(Status::Unavailable { reason: STOPPED.to_owned() }),
        }
    }
    let finished = match &st.run {
        Some(run) => match run.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                None
            }
            Err(TryRecvError::Disconnected) => Some(Err(STOPPED.to_owned())),
        },
        None => None,
    };
    if let Some(body) = finished
        && let Some(run) = st.run.take()
    {
        // A failure may mean the sign-in changed; the user just acted, so look again now.
        let failed = body.is_err();
        push_answer(&mut st.history, Answer::new(run.heading, run.doc, body));
        if failed && !matches!(st.status, StatusSlot::Checking(_)) {
            start_status_check(st, ctx);
        }
    }
}

/// Ask Claude Code about its sign-in on a worker thread.
fn start_status_check(st: &mut AssistantState, ctx: &egui::Context) {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("mirai-assistant-status".into()).spawn(move || {
        let status = std::panic::catch_unwind(pdfcraft_ai::status).unwrap_or_else(|_| Status::Unavailable { reason: STOPPED.to_owned() });
        let _ = tx.send(status);
        ctx.request_repaint();
    });
    st.status = match spawned {
        Ok(_) => StatusSlot::Checking(rx),
        Err(e) => StatusSlot::Known(Status::Unavailable { reason: e.to_string() }),
    };
}

/// Draw the panel body. Returns a 0-based page to go to when a citation was clicked.
fn draw(app: &mut PdfCraftApp, st: &mut AssistantState, ui: &mut egui::Ui, t: &Tokens) -> Option<usize> {
    let mut go = None;
    let mut send = false;
    let mut cancel = false;
    let mut copy = None;
    let mut remove = None;
    let mut new_conversation = false;
    let mut chip_clicked = None;
    // Unchecked is usable: `ask` verifies the sign-in itself before sending anything.
    let ready = matches!(&st.status, StatusSlot::Unchecked) || matches!(&st.status, StatusSlot::Known(s) if s.is_ready());
    let doc = app.active_ids();
    let running = st.run.is_some();
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        super::tools::clear_of_scrollbar(ui);
        status_card(app, st, ui, t);
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(tl!(
                "Only the pages you choose are sent to Claude through your Claude Code sign-in. Nothing is sent until you press a button."
            ))
            .size(12.0)
            .color(t.text_muted),
        );
        ui.add_space(8.0);

        widgets::section_title(ui, tl!("Pages to read"));
        widgets::radio_value(ui, &mut st.scope, Scope::Current, tl!("Current page"));
        widgets::radio_value(ui, &mut st.scope, Scope::All, tl!("All pages (up to 500)"));
        ui.add_space(8.0);

        let usable = ready && doc.is_some() && !running;
        if doc.is_none() {
            ui.label(egui::RichText::new(tl!("Open a PDF to use the assistant.")).size(12.0).color(t.text_muted));
        }
        let shown: Vec<usize> = match doc {
            Some((_, id)) => st.history.iter().enumerate().filter(|(_, a)| a.doc == id).map(|(i, _)| i).collect(),
            None => Vec::new(),
        };
        ui.add_enabled_ui(usable, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut st.mode, Mode::Ask, tl!("Ask"));
                ui.selectable_value(&mut st.mode, Mode::Summarize, tl!("Summarize"));
                ui.selectable_value(&mut st.mode, Mode::Translate, tl!("Translate"));
                ui.selectable_value(&mut st.mode, Mode::Explain, tl!("Explain"));
            });
            ui.add_space(4.0);
            match st.mode {
                Mode::Ask => {
                    let field = ui.add(
                        widgets::block(&mut st.question)
                            .id_salt("mirai_assistant_question")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .hint_text(tl!("Type your question")),
                    );
                    if st.focus_question {
                        field.request_focus();
                        st.focus_question = false;
                    }
                    let ctrl_enter = field.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
                    if ctrl_enter && !st.question.trim().is_empty() {
                        send = true;
                    }
                }
                Mode::Translate => {
                    ui.add(widgets::line(&mut st.language).desired_width(f32::INFINITY).hint_text(tl!("Target language")));
                }
                Mode::Summarize | Mode::Explain => {}
            }
            ui.add_space(4.0);
            let can_send = st.mode != Mode::Ask || !st.question.trim().is_empty();
            ui.add_enabled_ui(can_send, |ui| {
                if widgets::pill_button(ui, tl!("Send"), true).clicked() {
                    send = true;
                }
            });
            if shown.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for chip in CHIPS {
                        if ui.small_button(tl!(chip_label(chip))).clicked() {
                            chip_clicked = Some(chip);
                        }
                    }
                });
            }
        });
        ui.add_space(8.0);

        if let Some(run) = &st.run {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                let time = elapsed_label(run.started.elapsed().as_secs());
                ui.label(crate::i18n::fmt(tl!("{name} is working… {time}"), &[("name", pdfcraft_ai::ASSISTANT_NAME), ("time", &time)]));
            });
            let n = run.pages.to_string();
            ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Reading {n} pages"), &[("n", &n)])).size(12.0).color(t.text_muted));
            if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
                cancel = true;
            }
            ui.add_space(8.0);
        }

        for &i in &shown {
            let Some(answer) = st.history.get(i) else { continue };
            ui.push_id(i, |ui| {
                ui.add_space(4.0);
                let you = crate::i18n::fmt(tl!("You: {text}"), &[("text", &answer.heading)]);
                ui.label(egui::RichText::new(you).size(12.0).color(t.text_muted));
                ui.add_space(2.0);
                match &answer.body {
                    Ok(text) => {
                        if let Some(page) = answer_text(ui, t, text, &answer.cites) {
                            go = Some(page);
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if widgets::pill_button(ui, tl!("Copy"), false).clicked() {
                                copy = Some(text.clone());
                            }
                            if ui.small_button(tl!("Remove")).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    Err(message) => {
                        let color = if matches!(t.kind, ThemeKind::Dark) {
                            egui::Color32::from_rgb(0xFF, 0x8A, 0x80)
                        } else {
                            egui::Color32::from_rgb(0xB0, 0x1E, 0x1E)
                        };
                        ui.label(egui::RichText::new(message).color(color));
                        if ui.small_button(tl!("Remove")).clicked() {
                            remove = Some(i);
                        }
                    }
                }
                ui.add_space(6.0);
                ui.separator();
            });
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(!running, |ui| {
            if ui.small_button(tl!("New conversation")).clicked() {
                new_conversation = true;
            }
        });
    });
    if let Some(text) = copy {
        ui.ctx().copy_text(text);
    }
    if let Some(chip) = chip_clicked {
        fill_question(st, tl!(chip_question(chip)));
    }
    if let Some(i) = remove {
        let _ = st.history.remove(i);
    }
    if new_conversation {
        st.history.clear();
        st.question.clear();
    }
    if cancel && let Some(run) = st.run.take() {
        run.cancel.store(true, Ordering::Relaxed);
    }
    if send
        && let Some((view, id)) = doc
        && st.run.is_none()
    {
        let mode = st.mode;
        start_run(app, st, ui.ctx(), view, id, mode);
    }
    go
}

/// A small coloured dot before a status line.
fn dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

fn status_card(app: &mut PdfCraftApp, st: &mut AssistantState, ui: &mut egui::Ui, t: &Tokens) {
    let mut check = false;
    let mut install = false;
    let green = egui::Color32::from_rgb(0x2E, 0xA0, 0x43);
    let amber = egui::Color32::from_rgb(0xD9, 0x9A, 0x1E);
    let red = egui::Color32::from_rgb(0xD6, 0x45, 0x45);
    egui::Frame::NONE.fill(t.card).corner_radius(egui::CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match &st.status {
            StatusSlot::Unchecked => {
                ui.horizontal(|ui| {
                    dot(ui, t.text_muted);
                    ui.label(tl!("Not checked yet"));
                });
            }
            StatusSlot::Checking(_) => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(tl!("Checking Claude Code…"));
                });
            }
            StatusSlot::Known(Status::Ready { plan }) => {
                ui.horizontal(|ui| {
                    dot(ui, green);
                    ui.label(tl!("Connected with your Claude account"));
                });
                if let Some(plan) = plan.as_deref().filter(|p| !p.trim().is_empty()) {
                    ui.label(egui::RichText::new(plan).size(12.0).color(t.text_muted));
                }
            }
            StatusSlot::Known(other) => {
                let color = if matches!(other, Status::Unavailable { .. }) { amber } else { red };
                ui.horizontal_top(|ui| {
                    dot(ui, color);
                    ui.label(other.advice());
                });
                if matches!(other, Status::NotInstalled) {
                    ui.add_space(4.0);
                    install = widgets::pill_button(ui, tl!("Get Claude Code"), true).clicked();
                }
            }
        }
        if !matches!(st.status, StatusSlot::Checking(_)) {
            ui.add_space(4.0);
            let label = if matches!(st.status, StatusSlot::Unchecked) { tl!("Check connection") } else { tl!("Check again") };
            ui.horizontal(|ui| {
                if icons::button(ui, "rotate-cw", 24.0, false, label).clicked() {
                    check = true;
                }
                ui.label(egui::RichText::new(label).size(12.0).color(t.text_muted));
            });
        }
    });
    if install {
        app.open_url(pdfcraft_ai::INSTALL_URL);
    }
    if check {
        start_status_check(st, ui.ctx());
    }
}

fn selected_pages(scope: Scope, current: usize, total: usize) -> Result<Vec<usize>, String> {
    match scope {
        Scope::Current if total > 0 => Ok(vec![current.min(total - 1)]),
        Scope::Current => Ok(Vec::new()),
        Scope::All if total > MAX_PAGES => {
            Err(tl!("All pages is limited to 500 pages. Nothing was sent; choose the current page instead.").to_owned())
        }
        Scope::All => Ok((0..total).collect()),
    }
}

/// The page text of the pages to read, collected inside the worker.
fn collect(src: ExportSource, pages: Vec<usize>, cancel: &AtomicBool) -> Result<Vec<(u32, String)>, String> {
    let mut exporter = Exporter::from_source(src);
    collect_pages(pages, cancel, |page| exporter.text(page))
}

fn collect_pages(
    pages: Vec<usize>,
    cancel: &AtomicBool,
    mut text_of: impl FnMut(usize) -> Result<String, String>,
) -> Result<Vec<(u32, String)>, String> {
    let mut out = Vec::new();
    let mut chars = 0usize;
    for page in pages {
        if cancel.load(Ordering::Relaxed) {
            return Err(tl!("Cancelled. Nothing was sent.").to_owned());
        }
        let number = page.saturating_add(1);
        let text = text_of(page).map_err(|error| {
            crate::i18n::fmt(
                tl!("Could not read page {page}: {error}. Nothing was sent. Try another page or recognize its text with OCR."),
                &[("page", &number.to_string()), ("error", &error)],
            )
        })?;
        let number = u32::try_from(number).map_err(|_| tl!("The page number is too large. Nothing was sent.").to_owned())?;
        // The AI prompt counts the page wrapper as well as trimmed text. Refuse before its
        // truncation would omit part of the selection without a reliable UI notice.
        let wrapper = PAGE_WRAPPER_CHARS.saturating_add(number.checked_ilog10().map_or(1, |d| d as usize + 1));
        chars = chars.saturating_add(wrapper).saturating_add(text.trim().chars().count());
        if chars > pdfcraft_ai::MAX_DOCUMENT_CHARS {
            return Err(
                tl!("The selected text is too long for one request. Nothing was sent; try the current page or a shorter document.").to_owned()
            );
        }
        out.push((number, text));
    }
    Ok(out)
}

fn start_run(app: &mut PdfCraftApp, st: &mut AssistantState, ctx: &egui::Context, view: usize, id: DocId, mode: Mode) {
    let Some(doc) = app.session.get(id) else { return };
    let title = Some(doc.display_name()).filter(|n| !n.trim().is_empty());
    let current = app.views.get(view).map_or(0, |v| v.current);
    let (task, heading) = match mode {
        Mode::Summarize => (Task::Summarize, tl!("Summary").to_owned()),
        Mode::Explain => (Task::Explain, tl!("Explanation").to_owned()),
        Mode::Translate => {
            let language = st.language.trim();
            let language = if language.is_empty() { "English".to_owned() } else { language.to_owned() };
            let heading = crate::i18n::fmt(tl!("Translation into {language}"), &[("language", &language)]);
            (Task::Translate(language), heading)
        }
        Mode::Ask => {
            let question = st.question.trim().to_owned();
            (Task::Ask(question.clone()), question)
        }
    };
    // A document that forbids copying its text is not sent anywhere, as `text_extract` and the
    // `assistant_ask` tool refuse it too.
    if !doc.allows_copying() {
        let body = Err(tl!("This document does not allow copying its text, so the assistant can't read it.").to_owned());
        push_answer(&mut st.history, Answer::new(heading, id, body));
        return;
    }
    let source = doc.export_source();
    let pages = match selected_pages(st.scope, current, source.pages) {
        Ok(pages) => pages,
        Err(message) => {
            push_answer(&mut st.history, Answer::new(heading, id, Err(message)));
            return;
        }
    };
    let count = pages.len();
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let worker_cancel = cancel.clone();
    let ctx = ctx.clone();
    let lang = crate::i18n::current();
    let spawned = std::thread::Builder::new().name("mirai-assistant".into()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::i18n::set_current(lang);
            pdfcraft_ai::ensure_ready().map_err(|e| e.to_string())?;
            let pages = collect(source, pages, &worker_cancel)?;
            pdfcraft_ai::ask(&Request { task, title, pages }, Some(&worker_cancel)).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|_| Err(STOPPED.to_owned()));
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    match spawned {
        Ok(_) => st.run = Some(Run { rx, cancel, pages: count, heading, doc: id, started: web_time::Instant::now() }),
        Err(e) => push_answer(&mut st.history, Answer::new(heading, id, Err(e.to_string()))),
    }
}

/// Draw the answer as selectable text with its `[p. N]` citations as links. Returns the 0-based
/// page of a clicked citation.
fn answer_text(ui: &mut egui::Ui, t: &Tokens, text: &str, cites: &[(std::ops::Range<usize>, u32)]) -> Option<usize> {
    let mut go = None;
    let mut offset = 0usize;
    // Citations are sorted and never overlap, so one cursor walks them across all the lines.
    let mut next = 0usize;
    for line in text.split('\n') {
        let end = offset + line.len();
        if line.trim().is_empty() {
            ui.add_space(6.0);
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let mut at = offset;
                // Skip what lies before this line (a citation that straddled the last one is dropped).
                while cites.get(next).is_some_and(|(r, _)| r.start < offset) {
                    next += 1;
                }
                // A citation that starts here but ends past the line is not drawn as a link.
                while let Some((range, page)) = cites.get(next).filter(|(r, _)| r.end <= end) {
                    next += 1;
                    if let Some(before) = text.get(at..range.start) {
                        ui.label(egui::RichText::new(before).color(t.text));
                    }
                    let shown = text.get(range.clone()).unwrap_or("[p.]");
                    if ui.link(shown).on_hover_text(tl!("Go to this page")).clicked() {
                        go = Some(page.saturating_sub(1) as usize);
                    }
                    at = range.end;
                }
                if let Some(rest) = text.get(at..end) {
                    ui.label(egui::RichText::new(rest).color(t.text));
                }
            });
        }
        // The line break itself.
        offset = end.saturating_add(1);
    }
    go
}

/// The `[p. N]` citations in `text`: the byte range of each and its first page number (counting
/// from 1). `[p.N]` and `[p. N–M]` or `[p. N-M]` count too. A malformed citation, page 0 or a
/// number too large for a page is skipped.
fn citations(text: &str) -> Vec<(std::ops::Range<usize>, u32)> {
    let mut found = Vec::new();
    let mut from = 0usize;
    while let Some(open) = text.get(from..).and_then(|s| s.find("[p.")).map(|i| i + from) {
        from = open + 1;
        let Some(after) = text.get(open + 3..) else { continue };
        let Some(close) = after.find(']') else { break };
        let Some(inside) = after.get(..close) else { continue };
        // At most a short run of digits, spaces and range or list marks.
        if inside.chars().count() > 24 || !inside.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '\u{2013}' | '\u{2014}' | ',')) {
            continue;
        }
        let digits: String = inside.trim_start().chars().take_while(char::is_ascii_digit).collect();
        let Ok(page) = digits.parse::<u32>() else { continue };
        if page == 0 {
            continue;
        }
        let end = open + 3 + close + 1;
        found.push((open..end, page));
        from = end;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_extraction_is_not_silently_discarded() {
        let src = ExportSource { bytes: Arc::new(Vec::new()), config: Default::default(), pages: 1, sizes: vec![] };
        let error = collect(src, vec![0], &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("page 1"), "{error}");
        assert!(error.contains("Nothing was sent"), "{error}");
        let error = collect_pages(vec![0, 1, 2], &AtomicBool::new(false), |page| {
            if page == 1 { Err("damaged text layer".into()) } else { Ok("readable text".into()) }
        })
        .unwrap_err();
        assert!(error.contains("page 2") && error.contains("damaged text layer") && error.contains("Nothing was sent"), "{error}");
    }

    #[test]
    fn all_pages_does_not_silently_select_only_the_first_500() {
        let error = selected_pages(Scope::All, 0, MAX_PAGES + 1).unwrap_err();
        assert!(error.contains("500") && error.contains("Nothing was sent"), "{error}");
        assert_eq!(selected_pages(Scope::All, 0, MAX_PAGES).unwrap().len(), MAX_PAGES);
        assert_eq!(selected_pages(Scope::Current, MAX_PAGES, MAX_PAGES + 1).unwrap(), [MAX_PAGES]);
    }

    #[test]
    fn text_limit_refuses_partial_selection_including_page_wrapper() {
        let wrapper = "<page n=\"1\">\n\n</page>\n".chars().count();
        let fits = "x".repeat(pdfcraft_ai::MAX_DOCUMENT_CHARS - wrapper);
        let cancel = AtomicBool::new(false);
        assert!(collect_pages(vec![0], &cancel, |_| Ok(fits.clone())).is_ok());
        let error = collect_pages(vec![0, 1], &cancel, |_| Ok(fits.clone())).unwrap_err();
        assert!(error.contains("too long") && error.contains("Nothing was sent"), "{error}");
        let error = collect_pages(vec![0], &cancel, |_| Ok(format!("{fits}x"))).unwrap_err();
        assert!(error.contains("too long") && error.contains("Nothing was sent"), "{error}");
    }

    #[test]
    fn cancelled_extraction_never_returns_partial_text() {
        let cancel = AtomicBool::new(false);
        let error = collect_pages(vec![0, 1], &cancel, |page| {
            if page == 0 {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok("text".into())
        })
        .unwrap_err();
        assert!(error.contains("Nothing was sent"), "{error}");
    }

    fn answer(text: &str) -> Answer {
        Answer::new(String::new(), DocId(0), Ok(text.to_owned()))
    }

    #[test]
    fn elapsed_label_formats_seconds_and_minutes() {
        assert_eq!(elapsed_label(0), "0 s");
        assert_eq!(elapsed_label(8), "8 s");
        assert_eq!(elapsed_label(59), "59 s");
        assert_eq!(elapsed_label(60), "1 min 00 s");
        assert_eq!(elapsed_label(65), "1 min 05 s");
        assert_eq!(elapsed_label(725), "12 min 05 s");
        assert!(!elapsed_label(u64::MAX).is_empty());
    }

    #[test]
    fn history_keeps_at_most_eight_entries_newest_last() {
        let mut h = VecDeque::new();
        for i in 0..12 {
            push_answer(&mut h, answer(&i.to_string()));
        }
        assert_eq!(h.len(), MAX_HISTORY);
        assert!(matches!(h.front().map(|a| &a.body), Some(Ok(s)) if s == "4"));
        assert!(matches!(h.back().map(|a| &a.body), Some(Ok(s)) if s == "11"));
    }

    #[test]
    fn history_drops_oldest_over_the_byte_limit() {
        let mut h = VecDeque::new();
        let big = "x".repeat(MAX_HISTORY_BYTES / 2 + 1);
        push_answer(&mut h, answer(&big));
        push_answer(&mut h, answer(&big));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn a_single_huge_entry_is_kept_alone() {
        let huge = "y".repeat(MAX_HISTORY_BYTES * 2);
        let mut h = VecDeque::new();
        push_answer(&mut h, answer("small"));
        push_answer(&mut h, answer(&huge));
        assert_eq!(h.len(), 1);
        let mut empty = VecDeque::new();
        push_answer(&mut empty, answer(&huge));
        assert_eq!(empty.len(), 1);
    }

    #[test]
    fn a_chip_only_fills_the_question_and_never_starts_a_run() {
        for chip in CHIPS {
            assert!(!chip_label(chip).is_empty());
            let mut st = AssistantState { mode: Mode::Summarize, ..AssistantState::default() };
            fill_question(&mut st, chip_question(chip));
            assert_eq!(st.question, chip_question(chip));
            assert!(st.mode == Mode::Ask);
            assert!(st.focus_question);
            assert!(st.run.is_none());
            assert!(st.history.is_empty());
        }
    }

    #[test]
    fn finds_ascii_citations() {
        let text = "See [p. 3] and [p.12] too.";
        let found = citations(text);
        assert_eq!(found.iter().map(|(_, p)| *p).collect::<Vec<_>>(), [3, 12]);
        assert_eq!(&text[found[0].0.clone()], "[p. 3]");
        assert_eq!(&text[found[1].0.clone()], "[p.12]");
    }

    #[test]
    fn a_range_links_to_its_first_page() {
        let text = "x [p. 4\u{2013}6] y [p. 7-9]";
        let found = citations(text);
        assert_eq!(found.iter().map(|(_, p)| *p).collect::<Vec<_>>(), [4, 7]);
        assert_eq!(&text[found[0].0.clone()], "[p. 4\u{2013}6]");
    }

    #[test]
    fn multibyte_text_around_a_citation_keeps_byte_ranges_valid() {
        let text = "日本語の要約 [p. 2] 終わり é [p. 5]";
        let found = citations(text);
        assert_eq!(found.len(), 2);
        for (range, _) in &found {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        assert_eq!(&text[found[0].0.clone()], "[p. 2]");
        assert_eq!(found[1].1, 5);
    }

    #[test]
    fn malformed_citations_are_skipped() {
        assert!(citations("[p. ]").is_empty());
        assert!(citations("[p. abc]").is_empty());
        assert!(citations("[p. 0]").is_empty());
        assert!(citations("[p. 3").is_empty());
        assert!(citations("[p.").is_empty());
        assert!(citations("").is_empty());
        assert_eq!(citations("[p. ] [p. 8]").len(), 1);
    }

    #[test]
    fn a_page_number_too_large_is_skipped_without_panicking() {
        assert!(citations("[p. 99999999999]").is_empty());
        assert_eq!(citations("[p. 99999999999] [p. 2]").iter().map(|(_, p)| *p).collect::<Vec<_>>(), [2]);
    }
}
