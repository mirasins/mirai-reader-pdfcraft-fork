//! pdfcraft-ai — L4 Mirai Assistant.
//!
//! The assistant answers questions about a document through **the official Claude Code binary and
//! the sign-in the user already made with it** (AGENTS.md, "Asistente Mirai: autenticación"). This
//! crate never reads, copies or stores a token, never falls back to an API key, another provider or
//! a server of ours. Sending is currently disabled: a separate `claude auth status` cannot prove
//! the effective authentication and hooks of the later prompt process. Managed settings can
//! override local settings and managed hooks cannot be disabled locally. Until both can be
//! established for the process receiving the document, the assistant fails closed.
//!
//! Nothing here starts on its own: the app and the automation tools call it only when the user asks.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod prompt;
mod run;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub use prompt::{MAX_DOCUMENT_CHARS, Request, Task, build_prompt};

/// The assistant's name, shown wherever the interface names it.
pub const ASSISTANT_NAME: &str = "Mirai Assistant";

/// Where to get Claude Code, for the "not installed" message.
pub const INSTALL_URL: &str = "https://claude.com/claude-code";

/// Variables that would make Claude Code use an API key, a gateway or a cloud provider instead of
/// the user's Claude account. Removing these from the child environment alone is not sufficient:
/// settings and managed policy may reintroduce them. Sending remains disabled for this reason.
pub const BLOCKED_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_BASE_URL",
    "ANTHROPIC_FOUNDRY_BASE_URL",
    "ANTHROPIC_FOUNDRY_RESOURCE",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// The only sign-in method accepted: the user's Claude account, as `claude auth status` names it.
const ALLOWED_AUTH_METHOD: &str = "claude.ai";
/// The only API provider accepted: Anthropic's own service.
const ALLOWED_PROVIDER: &str = "firstParty";

/// How long one answer may take.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(300);
/// The most output read from Claude Code; past it the reply is refused rather than kept.
const OUTPUT_CAP: u64 = 8 * 1024 * 1024;

/// Whether the assistant can be used, and if not, what the user has to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No `claude` binary was found.
    NotInstalled,
    /// Claude Code is installed but not signed in.
    NotSignedIn,
    /// Claude Code is signed in with a method the assistant doesn't use (an API key, a gateway, a
    /// cloud provider). `method` is what Claude Code reported, for the message.
    Disallowed { method: String },
    /// Claude Code could not be asked (it failed, timed out or answered something unreadable).
    Unavailable { reason: String },
    /// Signed in with a Claude account. `plan` is the subscription Claude Code reported, if any.
    Ready { plan: Option<String> },
}

impl Status {
    pub fn is_ready(&self) -> bool {
        matches!(self, Status::Ready { .. })
    }

    /// A short machine-readable name, for the automation tools.
    pub fn code(&self) -> &'static str {
        match self {
            Status::NotInstalled => "not_installed",
            Status::NotSignedIn => "not_signed_in",
            Status::Disallowed { .. } => "disallowed_method",
            Status::Unavailable { .. } => "unavailable",
            Status::Ready { .. } => "ready",
        }
    }

    /// What the user should do next, in English (the interface translates its own wording).
    pub fn advice(&self) -> String {
        match self {
            Status::NotInstalled => format!(
                "{ASSISTANT_NAME} needs Claude Code. Install it from {INSTALL_URL}, then run `claude` in a terminal once and sign in with your Claude account."
            ),
            Status::NotSignedIn => {
                "Claude Code is not signed in. Run `claude` in a terminal and sign in with your Claude account (/login).".to_owned()
            }
            Status::Disallowed { method } => format!(
                "Claude Code is set up to use `{method}`. {ASSISTANT_NAME} only uses a Claude account sign-in: run `claude` in a terminal, sign in with /login, and remove the API key or provider setting from Claude Code's settings."
            ),
            Status::Unavailable { reason } => format!("Claude Code could not be asked: {reason}"),
            Status::Ready { .. } => format!("{ASSISTANT_NAME} is ready."),
        }
    }
}

/// Why an answer could not be produced. Every variant says what to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiError {
    /// The assistant isn't usable; the status says why.
    NotReady(Status),
    /// The request was empty or malformed.
    InvalidRequest(String),
    /// The user cancelled.
    Cancelled,
    /// Claude Code took longer than the limit.
    TimedOut,
    /// Claude Code ran but reported an error, or replied with something unreadable.
    Failed(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::NotReady(s) => f.write_str(&s.advice()),
            AiError::InvalidRequest(m) => write!(f, "invalid request: {m}"),
            AiError::Cancelled => f.write_str("cancelled"),
            AiError::TimedOut => write!(f, "Claude Code did not answer within {} seconds; try a shorter page range", ANSWER_TIMEOUT.as_secs()),
            AiError::Failed(m) => write!(f, "Claude Code failed: {m}"),
        }
    }
}

impl std::error::Error for AiError {}

/// Read `claude auth status --json` output. Only a signed-in Claude account on Anthropic's own
/// service is [`Status::Ready`]; anything else (API key, gateway, cloud provider, an unknown
/// method a future version reports) is refused.
pub fn parse_auth_status(json: &str) -> Status {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Status::Unavailable { reason: "`claude auth status` printed something that is not JSON".into() };
    };
    let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_owned);
    if v.get("loggedIn").and_then(|x| x.as_bool()) != Some(true) {
        return Status::NotSignedIn;
    }
    let method = text("authMethod").unwrap_or_else(|| "unknown".into());
    let provider = text("apiProvider").unwrap_or_else(|| "unknown".into());
    if method != ALLOWED_AUTH_METHOD {
        return Status::Disallowed { method };
    }
    if provider != ALLOWED_PROVIDER {
        return Status::Disallowed { method: format!("{method} via {provider}") };
    }
    Status::Ready { plan: text("subscriptionType").or_else(|| text("planTier")) }
}

/// Read the JSON `claude -p --output-format json` prints and return the answer text.
pub fn parse_reply(json: &str) -> Result<String, AiError> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).map_err(|_| AiError::Failed("its reply was not the expected JSON".into()))?;
    let result = v.get("result").and_then(|x| x.as_str());
    if v.get("is_error").and_then(|x| x.as_bool()) == Some(true) {
        let why = result.or_else(|| v.get("subtype").and_then(|x| x.as_str())).unwrap_or("unknown error");
        return Err(AiError::Failed(why.to_owned()));
    }
    match result {
        Some(text) if !text.trim().is_empty() => Ok(text.to_owned()),
        _ => Err(AiError::Failed("it returned an empty answer".into())),
    }
}

/// A Claude Code candidate on this machine. A PATH entry alone does not establish its provenance.
#[derive(Clone, Debug)]
pub struct ClaudeCode {
    program: PathBuf,
}

impl ClaudeCode {
    /// Find `claude` on PATH, then in the folders Claude Code's own installers use (a desktop app
    /// started from the dock or Start menu often doesn't see the shell's PATH).
    pub fn locate() -> Option<Self> {
        let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        if let Some(home) = &home {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join(".claude").join("local"));
        }
        if cfg!(windows) {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                dirs.push(PathBuf::from(appdata).join("npm"));
            }
        } else {
            dirs.push(PathBuf::from("/opt/homebrew/bin"));
            dirs.push(PathBuf::from("/usr/local/bin"));
        }
        dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file()).map(|program| ClaudeCode { program })
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Report whether a request can safely be made. Currently this is unavailable even if Claude
    /// Code is installed: its effective prompt-process configuration cannot be established.
    pub fn status(&self) -> Status {
        self.status_cancellable(None).unwrap_or_else(|| Status::Unavailable { reason: "cancelled".into() })
    }

    /// Like [`status`](Self::status); `None` means `cancel` was set.
    fn status_cancellable(&self, cancel: Option<&AtomicBool>) -> Option<Status> {
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            return None;
        }
        // A separate auth status can differ from `-p`, and managed hooks cannot be disabled by
        // local flags. Do not start either process until the effective prompt configuration can
        // be verified without reading credentials or changing the user's settings.
        Some(Status::Unavailable {
            reason: "sending is disabled because the effective authentication and managed hooks of the prompt process cannot be verified".into(),
        })
    }

    /// Answer `request`. Currently refuses to send because the prompt process's effective
    /// authentication and managed hooks cannot be verified. A pre-set `cancel` returns
    /// [`AiError::Cancelled`].
    pub fn run(&self, request: &Request, cancel: Option<&AtomicBool>) -> Result<String, AiError> {
        let (system, user) = build_prompt(request)?;
        let status = self.status_cancellable(cancel).ok_or(AiError::Cancelled)?;
        if !status.is_ready() {
            return Err(AiError::NotReady(status));
        }
        let mut cmd = run::command(&self.program);
        cmd.args([
            "-p",
            "--output-format",
            "json",
            // These flags alone do not control managed hooks or prove authentication; the
            // fail-closed status gate above currently prevents this command from being run.
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--system-prompt",
            system.as_str(),
        ]);
        let out = run::run(cmd, user.as_bytes(), ANSWER_TIMEOUT, OUTPUT_CAP, cancel).map_err(|e| match e {
            run::RunError::Spawn(e) => AiError::Failed(format!("it could not be started ({e})")),
            run::RunError::TimedOut => AiError::TimedOut,
            run::RunError::Cancelled => AiError::Cancelled,
            run::RunError::TooLarge => AiError::Failed("its reply was larger than expected".into()),
        })?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.success && stdout.trim().is_empty() {
            // No JSON at all: the first line of its error output says more than "not JSON".
            let stderr = String::from_utf8_lossy(&out.stderr);
            let first = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("it exited with an error and no details");
            return Err(AiError::Failed(first.chars().take(300).collect()));
        }
        parse_reply(&stdout)
    }
}

/// The assistant's status on this machine.
pub fn status() -> Status {
    match ClaudeCode::locate() {
        Some(c) => c.status(),
        None => Status::NotInstalled,
    }
}

/// Answer `request` with the Claude Code found on this machine.
pub fn ask(request: &Request, cancel: Option<&AtomicBool>) -> Result<String, AiError> {
    ClaudeCode::locate().ok_or(AiError::NotReady(Status::NotInstalled))?.run(request, cancel)
}

#[cfg(test)]
mod tests;
