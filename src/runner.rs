use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use thiserror::Error;

/// Returned (inside `anyhow::Error`) when a run was stopped through its
/// `RenderControl`. Callers check with `error.downcast_ref::<Cancelled>()`.
#[derive(Debug, Error)]
#[error("cancelled")]
pub struct Cancelled;

/// Shared handle for one ffmpeg run: the owner can cancel it and watch how
/// far the encoder has got. Clones refer to the same run.
#[derive(Debug, Clone, Default)]
pub struct RenderControl {
    cancel: Arc<AtomicBool>,
    progress_us: Arc<AtomicU64>,
}

impl RenderControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Output time ffmpeg has encoded so far, in seconds.
    pub fn progress_seconds(&self) -> f64 {
        self.progress_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }
}

const STDERR_TAIL: usize = 64 * 1024;

/// How long to wait for the output pipes to drain once the child is gone.
const PIPE_DRAIN_WAIT: Duration = Duration::from_secs(1);

/// Runs `command` to completion while honouring `control`. ffmpeg must have
/// been given `-progress pipe:1` for progress to be reported. Returns the
/// tail of stderr on success; on failure the error carries the exit status and
/// stderr, and a cancelled run returns `Cancelled` after the child is killed
/// and reaped.
pub fn run(mut command: Command, control: &RenderControl) -> Result<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to launch {}",
            command.get_program().to_string_lossy()
        )
    })?;

    let stdout = child.stdout.take().context("child has no stdout pipe")?;
    let mut stderr = child.stderr.take().context("child has no stderr pipe")?;

    let progress = Arc::clone(&control.progress_us);
    let (progress_done_tx, progress_done) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
            if let Some(value) = line.strip_prefix("out_time_us=") {
                if let Ok(micros) = value.trim().parse::<i64>() {
                    progress.store(micros.max(0) as u64, Ordering::Relaxed);
                }
            }
        }
        let _ = progress_done_tx.send(());
    });
    // Drain stderr continuously so a chatty child can never block on a full
    // pipe, but keep only the tail.
    let (stderr_tx, stderr_done) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut tail: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(read) = stderr.read(&mut chunk) {
            if read == 0 {
                break;
            }
            tail.extend_from_slice(&chunk[..read]);
            if tail.len() > STDERR_TAIL {
                tail.drain(..tail.len() - STDERR_TAIL);
            }
        }
        let _ = stderr_tx.send(String::from_utf8_lossy(&tail).into_owned());
    });

    let outcome = loop {
        if control.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(Ok(status)),
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Some(Err(error));
            }
        }
    };

    // Normally both pipes close when the child exits. A grandchild that
    // inherited them (a shell that did not exec its command) can keep them
    // open, so wait only briefly and let the reader threads finish on their
    // own rather than hanging a cancel for as long as that process lives.
    let _ = progress_done.recv_timeout(PIPE_DRAIN_WAIT);
    let stderr_text = stderr_done
        .recv_timeout(PIPE_DRAIN_WAIT)
        .unwrap_or_default();

    match outcome {
        None => Err(Cancelled.into()),
        Some(Err(error)) => Err(error).context("failed while waiting for the encoder"),
        Some(Ok(status)) if status.success() => Ok(stderr_text),
        Some(Ok(status)) => anyhow::bail!("{status}: {}", stderr_text.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn success_returns_stderr_and_reports_progress() {
        let control = RenderControl::new();
        let script = "echo out_time_us=2500000; echo warn >&2";
        let stderr = run(shell(script), &control).expect("run");
        assert_eq!(stderr.trim(), "warn");
        assert!((control.progress_seconds() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn failure_carries_status_and_stderr() {
        let error = run(shell("echo boom >&2; exit 3"), &RenderControl::new())
            .expect_err("must fail")
            .to_string();
        assert!(error.contains("boom"), "{error}");
        assert!(error.contains('3'), "{error}");
    }

    #[test]
    fn cancel_kills_a_long_running_child_promptly() {
        let control = RenderControl::new();
        let remote = control.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            remote.cancel();
        });
        let started = Instant::now();
        // `;` keeps sh from exec'ing sleep, so killing sh leaves sleep alive
        // and holding the pipes open: cancel must not wait for it.
        let error =
            run(shell("sleep 30; echo unreachable"), &control).expect_err("must be cancelled");
        canceller.join().expect("canceller");
        assert!(error.downcast_ref::<Cancelled>().is_some(), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn huge_stderr_does_not_deadlock() {
        let script = "head -c 3000000 /dev/zero | tr '\\0' x >&2";
        let stderr = run(shell(script), &RenderControl::new()).expect("run");
        assert!(stderr.len() <= STDERR_TAIL);
    }
}
