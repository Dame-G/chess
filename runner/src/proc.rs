//! Child processes that take their own children with them: a Job Object on Windows, a process group elsewhere.
//! Cancelling a build or a match must not leave a compiler or an engine running.
//!
//! Dropping a [`Child`] kills its whole tree. On Windows the Job Object also dies with the runner however it ends; a
//! Unix process group does not, so `chess serve` stops on Ctrl-C or SIGTERM by returning, which drops every child.

use std::{
    ops::{Deref, DerefMut},
    path::Path,
    process::Stdio,
};

use anyhow::{Context, Result};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::mpsc,
};

/// A running child process and its descendants, all killed when it is dropped.
pub struct Child(Box<dyn ChildWrapper>);

impl Deref for Child {
    type Target = dyn ChildWrapper;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for Child {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

impl Drop for Child {
    /// `KillOnDrop` takes the whole Job Object on Windows but only the group's leader on Unix, which would leave rustc
    /// running after a cancelled build and fastchess's engines after a cancelled test. Killing the group takes them
    /// all; after the child has exited it does nothing.
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

pub struct Spawn<'a> {
    pub program: &'a Path,
    pub args: &'a [&'a str],
    pub cwd: Option<&'a Path>,
    pub env: &'a [(&'a str, &'a str)],
    pub stdin: bool,
}

pub fn spawn(s: Spawn) -> Result<Child> {
    let mut cmd = CommandWrap::with_new(s.program, |c| {
        c.args(s.args).stdout(Stdio::piped()).stderr(Stdio::piped());
        c.stdin(if s.stdin { Stdio::piped() } else { Stdio::null() });
        if let Some(cwd) = s.cwd {
            c.current_dir(cwd);
        }
        for (k, v) in s.env {
            c.env(k, v);
        }
    });
    #[cfg(windows)]
    cmd.wrap(process_wrap::tokio::JobObject);
    #[cfg(unix)]
    cmd.wrap(process_wrap::tokio::ProcessGroup::leader());
    cmd.wrap(KillOnDrop);
    cmd.spawn().map(Child).with_context(|| format!("start {}", s.program.display()))
}

/// Every line the child writes, stdout and stderr interleaved as they arrive. The channel closes when both end.
pub fn lines(child: &mut Child) -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    if let Some(out) = child.stdout().take() {
        forward(out, tx.clone());
    }
    if let Some(err) = child.stderr().take() {
        forward(err, tx);
    }
    rx
}

fn forward(stream: impl AsyncRead + Unpin + Send + 'static, tx: mpsc::UnboundedSender<String>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

/// Runs a command to the end and returns its stdout, or an error with its output. Dropped early, it kills the command.
pub async fn output(program: &Path, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output().await.with_context(|| format!("run {}", program.display()))?;
    if !out.status.success() {
        anyhow::bail!(
            "{} {} failed: {}",
            program.display(),
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
