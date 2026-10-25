//! Child processes that die with their parent and take their own children with them: a Job Object on Windows, a
//! process group elsewhere. Cancelling a build or a match must not leave a compiler or an engine running.

use std::{path::Path, process::Stdio};

use anyhow::{Context, Result};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::mpsc,
};

pub type Child = Box<dyn ChildWrapper>;

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
    cmd.spawn().with_context(|| format!("start {}", s.program.display()))
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

/// Runs a command to the end and returns its stdout, or an error with its output.
pub async fn output(program: &Path, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd.unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .output()
        .await
        .with_context(|| format!("run {}", program.display()))?;
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
