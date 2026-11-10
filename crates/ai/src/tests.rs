use super::*;

fn req(task: Task, pages: &[(u32, &str)]) -> Request {
    Request { task, title: Some("contract.pdf".into()), pages: pages.iter().map(|(p, t)| (*p, (*t).to_owned())).collect() }
}

#[test]
fn a_claude_account_sign_in_is_ready() {
    let json = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","subscriptionType":"pro"}"#;
    assert_eq!(parse_auth_status(json), Status::Ready { plan: Some("pro".into()) });
}

#[test]
fn an_api_key_or_another_provider_is_refused() {
    let key = r#"{"loggedIn":true,"authMethod":"api_key","apiProvider":"firstParty"}"#;
    assert_eq!(parse_auth_status(key), Status::Disallowed { method: "api_key".into() });
    let bedrock = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"bedrock"}"#;
    assert!(matches!(parse_auth_status(bedrock), Status::Disallowed { .. }));
    // A method a future Claude Code reports, or none at all, is refused rather than trusted.
    let unknown = r#"{"loggedIn":true}"#;
    assert!(matches!(parse_auth_status(unknown), Status::Disallowed { .. }));
}

#[test]
fn signed_out_and_garbage_are_not_ready() {
    assert_eq!(parse_auth_status(r#"{"loggedIn":false}"#), Status::NotSignedIn);
    assert!(matches!(parse_auth_status("not json"), Status::Unavailable { .. }));
    assert!(!parse_auth_status("").is_ready());
}

#[test]
fn every_status_says_what_to_do() {
    for s in [
        Status::NotInstalled,
        Status::NotSignedIn,
        Status::Disallowed { method: "api_key".into() },
        Status::Unavailable { reason: "x".into() },
        Status::Ready { plan: None },
    ] {
        assert!(!s.advice().is_empty(), "{s:?}");
    }
    assert!(Status::NotInstalled.advice().contains(INSTALL_URL));
}

#[test]
fn replies_are_read_and_errors_reported() {
    assert_eq!(parse_reply(r#"{"type":"result","is_error":false,"result":"Hello [p. 1]"}"#), Ok("Hello [p. 1]".into()));
    assert!(matches!(parse_reply(r#"{"is_error":true,"result":"Credit balance is too low"}"#), Err(AiError::Failed(m)) if m.contains("Credit")));
    assert!(parse_reply(r#"{"is_error":false,"result":"  "}"#).is_err());
    assert!(parse_reply("<html>").is_err());
}

#[test]
fn the_document_goes_in_the_message_not_the_system_prompt() {
    let (system, user) = build_prompt(&req(Task::Summarize, &[(1, "Alpha"), (2, "Beta")])).unwrap();
    assert!(system.contains(ASSISTANT_NAME));
    assert!(!system.contains('\n'), "the system prompt is one command-line argument");
    assert!(!system.contains("Alpha"));
    assert!(user.contains("<page n=\"1\">\nAlpha\n</page>") && user.contains("<page n=\"2\">\nBeta\n</page>"));
    assert!(user.contains("contract.pdf"));
}

#[test]
fn empty_questions_and_textless_pages_are_refused() {
    assert!(matches!(build_prompt(&req(Task::Ask("  ".into()), &[(1, "x")])), Err(AiError::InvalidRequest(_))));
    assert!(matches!(build_prompt(&req(Task::Translate(String::new()), &[(1, "x")])), Err(AiError::InvalidRequest(_))));
    assert!(matches!(build_prompt(&req(Task::Summarize, &[(1, " "), (2, "")])), Err(AiError::InvalidRequest(m)) if m.contains("OCR")));
    let long = "q".repeat(5_000);
    assert!(build_prompt(&req(Task::Ask(long), &[(1, "x")])).is_err());
}

#[test]
fn long_documents_are_cut_at_a_page_and_say_so() {
    let page = "é".repeat(MAX_DOCUMENT_CHARS / 3);
    let pages: Vec<(u32, &str)> = (1..=5).map(|p| (p, page.as_str())).collect();
    let (_, user) = build_prompt(&req(Task::Summarize, &pages)).unwrap();
    assert!(user.contains("only the text up to page 2"), "{}", user.chars().take(200).collect::<String>());
    assert!(!user.contains("<page n=\"3\">"));
    // One page bigger than the limit is cut inside the page, at a character boundary.
    let huge = "ü".repeat(MAX_DOCUMENT_CHARS + 10);
    let (_, user) = build_prompt(&req(Task::Explain, &[(7, huge.as_str())])).unwrap();
    assert!(user.contains("up to page 7"));
    assert!(user.chars().count() < MAX_DOCUMENT_CHARS + 2_000);
}

#[test]
fn a_cancel_set_before_the_run_starts_no_process() {
    use std::sync::atomic::AtomicBool;
    let cancel = AtomicBool::new(true);
    let missing = std::env::temp_dir().join("pdfcraft-no-such-claude-program");
    let cmd = run::command(&missing);
    let r = run::run(cmd, b"x", Duration::from_secs(5), 1024, Some(&cancel));
    assert!(matches!(r, Err(run::RunError::Cancelled)));
    let claude = ClaudeCode { program: missing };
    assert_eq!(claude.run(&req(Task::Summarize, &[(1, "x")]), Some(&cancel)), Err(AiError::Cancelled));
}

#[test]
fn ensure_ready_is_ok_or_not_ready_and_nothing_else() {
    // Passes with or without Claude Code on this machine.
    assert!(matches!(ensure_ready(), Ok(()) | Err(AiError::NotReady(_))));
}

// This fake CLI never reads real settings or credentials. A ready `auth status` in a separate
// process cannot prove that the later prompt process won't load managed hooks or a different auth.
fn fake_claude() -> (ClaudeCode, std::path::PathBuf) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("pdfcraft-ai-test-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let marker = dir.join("prompt-started");
    #[cfg(windows)]
    let (program, script) = (
        dir.join("claude.cmd"),
        format!(
            "@echo off\r\nif \"%1\"==\"auth\" (echo {{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"apiProvider\":\"firstParty\"}}& exit /b 0)\r\necho started > \"{}\"\r\necho {{\"is_error\":false,\"result\":\"fake\"}}\r\n",
            marker.display()
        ),
    );
    #[cfg(not(windows))]
    let (program, script) = (
        dir.join("claude"),
        format!(
            "#!/bin/sh\nif [ \"$1\" = auth ]; then echo '{{\"loggedIn\":true,\"authMethod\":\"claude.ai\",\"apiProvider\":\"firstParty\"}}'; exit 0; fi\necho started > '{}'\necho '{{\"is_error\":false,\"result\":\"fake\"}}'\n",
            marker.display()
        ),
    );
    std::fs::write(&program, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    (ClaudeCode { program }, marker)
}

#[test]
fn separate_auth_status_does_not_prove_prompt_configuration() {
    let (claude, marker) = fake_claude();
    assert!(matches!(claude.status(), Status::Unavailable { .. }));
    assert!(!marker.exists());
}

#[test]
fn document_is_not_sent_without_effective_configuration_proof() {
    let (claude, marker) = fake_claude();
    let result = claude.run(&req(Task::Summarize, &[(1, "synthetic private text")]), None);
    assert!(matches!(result, Err(AiError::NotReady(Status::Unavailable { .. }))));
    assert!(!marker.exists());
}

#[test]
fn pipe_test_helper() {
    match std::env::var("PDFCRAFT_AI_PIPE_TEST").as_deref() {
        Ok("parent") => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.args(["--exact", "tests::pipe_test_helper", "--nocapture"]);
            child.env("PDFCRAFT_AI_PIPE_TEST", "grandchild");
            // Deliberately leave a short-lived grandchild holding the inherited pipes after its
            // parent exits; waiting here would not exercise the regression.
            #[allow(clippy::zombie_processes)]
            child.spawn().unwrap();
        }
        Ok("grandchild") => std::thread::sleep(Duration::from_secs(2)),
        _ => {}
    }
}

#[test]
fn exited_child_with_inherited_pipes_still_obeys_deadline() {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "tests::pipe_test_helper", "--nocapture"]);
    cmd.env("PDFCRAFT_AI_PIPE_TEST", "parent");
    let start = std::time::Instant::now();
    let result = run::run(cmd, &[], Duration::from_millis(300), 1024 * 1024, None);
    assert!(matches!(result, Err(run::RunError::TimedOut)));
    assert!(start.elapsed() < Duration::from_millis(1500));
}

#[test]
fn cancellation_is_observed_while_descendant_keeps_pipes_open() {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "tests::pipe_test_helper", "--nocapture"]);
    cmd.env("PDFCRAFT_AI_PIPE_TEST", "parent");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let setter = std::sync::Arc::clone(&cancel);
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        setter.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let start = std::time::Instant::now();
    let result = run::run(cmd, &[], Duration::from_secs(5), 1024 * 1024, Some(&cancel));
    thread.join().unwrap();
    assert!(matches!(result, Err(run::RunError::Cancelled)));
    assert!(start.elapsed() < Duration::from_millis(1500));
}
