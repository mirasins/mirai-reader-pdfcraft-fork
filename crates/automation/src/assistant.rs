//! `assistant_ask`: Mirai Assistant answers about a document's pages through the user's own
//! Claude Code sign-in (the `pdfcraft-ai` crate). Arguments are checked before anything runs, so a
//! bad call never starts Claude Code.

use super::error::{Result, ToolError, failed};
use super::json::Args;
use super::{Automation, Value, json};

/// Pages sent when the call names none.
const DEFAULT_PAGES: usize = 500;

impl Automation {
    /// Check the arguments, collect the selected pages' text with the same extractor as
    /// `text_extract`, and ask the assistant. Nothing leaves the machine before the checks pass.
    pub(crate) fn assistant_ask(&mut self, a: &Args) -> Result<Value> {
        let task = match a.str("task")? {
            "summarize" => pdfcraft_ai::Task::Summarize,
            "ask" => {
                let question = a
                    .opt_str("question")?
                    .map(str::trim)
                    .filter(|q| !q.is_empty())
                    .ok_or_else(|| ToolError::InvalidArgs("task \"ask\" needs question: the question to answer from the pages".into()))?;
                pdfcraft_ai::Task::Ask(question.to_owned())
            }
            "translate" => {
                let language =
                    a.opt_str("language")?.map(str::trim).filter(|l| !l.is_empty()).ok_or_else(|| {
                        ToolError::InvalidArgs("task \"translate\" needs language: the language to translate into, e.g. French".into())
                    })?;
                pdfcraft_ai::Task::Translate(language.to_owned())
            }
            "explain" => pdfcraft_ai::Task::Explain,
            other => return Err(ToolError::InvalidArgs(format!("unknown task {other:?} (allowed: summarize, ask, translate, explain)"))),
        };

        let doc = self.doc_to_copy(a)?;
        let id = doc.id;
        let n = doc.info.pages.len();
        let title = Some(doc.name.trim().to_owned()).filter(|t| !t.is_empty());
        // Without a selection, at most the first DEFAULT_PAGES: pdfcraft-ai cuts the text at
        // MAX_DOCUMENT_CHARS anyway, so extracting thousands of pages would be wasted work.
        let pages = match a.opt_ints("pages")? {
            Some(_) => self.pages(a, "pages")?,
            None => (0..n.min(DEFAULT_PAGES)).collect(),
        };

        // Before reading any page text: it is wasted work when the assistant can't be used.
        pdfcraft_ai::ensure_ready().map_err(failed)?;
        let texts = self.page_texts(id, &pages)?;
        let mut sent = Vec::with_capacity(texts.len());
        for (p, text) in pages.iter().zip(texts) {
            let number = u32::try_from(p.saturating_add(1)).unwrap_or(u32::MAX);
            sent.push((number, text.plain_text()));
        }
        let pages_sent = sent.len();

        let request = pdfcraft_ai::Request { task, title, pages: sent };
        let answer = pdfcraft_ai::ask(&request, None).map_err(failed)?;
        Ok(json!({ "answer": answer, "pages_sent": pages_sent }))
    }
}
