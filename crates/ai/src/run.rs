//! Running a child process with a deadline, output cap and cancellation. The environment and
//! working directory are not an isolation boundary. The caller must establish safety before
//! sending document text. Arguments go to the program and input goes through stdin.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::BLOCKED_ENV;

/// How often a running child is checked for exit, timeout and cancellation.
const POLL: Duration = Duration::from_millis(50);

pub(crate) struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum RunError {
    Spawn(String),
    TimedOut,
    Cancelled,
    TooLarge,
}

/// A command for `program` with the API-key and provider variables removed and the working folder
/// set to a separate working folder. This is not a security boundary: the folder may contain
/// settings and Claude Code also loads managed settings. The caller must not send a prompt until
/// the effective prompt-process configuration is verified.
pub(crate) fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    for var in BLOCKED_ENV {
        cmd.env_remove(var);
    }
    cmd.current_dir(work_dir());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: a console window must not flash up over the app.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// `<temp>/pdfcraft-assistant`, created if missing; the temp folder itself if that fails. Neither
/// is guaranteed empty or private, so this is not a trusted configuration boundary.
fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("pdfcraft-assistant");
    match std::fs::create_dir_all(&dir) {
        Ok(()) => dir,
        Err(_) => std::env::temp_dir(),
    }
}

/// Run `cmd`, writing `input` to its stdin, until it exits, `timeout` passes or `cancel` is set.
/// Output past `cap` bytes is an error, not a truncated answer.
pub(crate) fn run(mut cmd: Command, input: &[u8], timeout: Duration, cap: u64, cancel: Option<&AtomicBool>) -> Result<Output, RunError> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err(RunError::Cancelled);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Spawn(e.to_string()))?;

    // Feed stdin and drain both pipes on their own threads, so a full pipe can't deadlock us.
    let writer = child.stdin.take().map(|mut stdin| {
        let input = input.to_vec();
        // A write error means the child exited early; its exit status tells the story.
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        })
    });
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        pipe.map(|p| {
            thread::spawn(move || {
                let mut buf = Vec::new();
                // One byte past the cap marks the output as too large.
                let _ = p.take(cap.saturating_add(1)).read_to_end(&mut buf);
                buf
            })
        })
    };
    let out_thread = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_thread = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(RunError::Spawn(e.to_string())),
        }
        let stop = if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            Some(RunError::Cancelled)
        } else if Instant::now() >= deadline {
            Some(RunError::TimedOut)
        } else {
            None
        };
        if let Some(why) = stop {
            let _ = child.kill();
            let _ = child.wait();
            break Err(why);
        }
        thread::sleep(POLL);
    };

    // On timeout or cancel, leave pipe threads detached: descendants can keep the pipes open
    // even after the direct child has exited. This does not terminate those descendants.
    let status = status?;
    while writer.as_ref().is_some_and(|h| !h.is_finished())
        || out_thread.as_ref().is_some_and(|h| !h.is_finished())
        || err_thread.as_ref().is_some_and(|h| !h.is_finished())
    {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(RunError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(RunError::TimedOut);
        }
        thread::sleep(POLL);
    }
    // A reader thread that panicked yields no output rather than taking the caller down.
    let join = |h: Option<thread::JoinHandle<Vec<u8>>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
    if let Some(w) = writer {
        let _ = w.join();
    }
    let stdout = join(out_thread);
    let stderr = join(err_thread);
    if stdout.len() as u64 > cap || stderr.len() as u64 > cap {
        return Err(RunError::TooLarge);
    }
    Ok(Output { success: status.success(), stdout, stderr })
}
