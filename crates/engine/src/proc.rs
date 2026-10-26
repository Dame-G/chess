//! Child processes that take their own children with them: a Job Object on Windows, a process group elsewhere.
//! Cancelling a build, a bench or a fastchess round must not leave a compiler, an engine or a game running.
//!
//! Dropping a [`Child`] kills its whole tree. On Windows the Job Object also dies with the server however it ends; a
//! Unix process group does not, so the server must stop by returning (on Ctrl-C or SIGTERM), which drops every child.

use std::{
    ffi::OsString,
    ops::{Deref, DerefMut},
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

use crate::Cancelled;

/// A program to run: what, with which arguments, where and with which environment.
#[derive(Clone, Debug)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    /// Variables set for the child, on top of this process's environment.
    pub env: Vec<(OsString, OsString)>,
    /// Variables taken out of the child's environment (such as `RUSTUP_TOOLCHAIN` when the server runs under cargo).
    pub env_remove: Vec<OsString>,
    /// Whether the child gets a pipe on stdin (engines do), or nothing.
    pub stdin: bool,
}

impl Command {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            env_remove: Vec::new(),
            stdin: false,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn env_remove(mut self, key: impl Into<OsString>) -> Self {
        self.env_remove.push(key.into());
        self
    }

    pub fn stdin(mut self, piped: bool) -> Self {
        self.stdin = piped;
        self
    }

    /// The command as a person would type it, for logs.
    pub fn display(&self) -> String {
        let mut s = self.program.display().to_string();
        for a in &self.args {
            s.push(' ');
            s.push_str(&a.to_string_lossy());
        }
        s
    }
}

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

impl Child {
    /// Kills the whole tree and waits for the child to be gone.
    pub async fn kill_tree(&mut self) -> Result<()> {
        let _ = self.0.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.0.wait()).await;
        Ok(())
    }

    /// Whether the child has exited (without waiting).
    pub fn exited(&mut self) -> bool {
        !matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for Child {
    /// `KillOnDrop` takes the whole Job Object on Windows but only the group's leader on Unix, which would leave rustc
    /// running after a cancelled build and fastchess's engines after a cancelled round. Killing the group takes them
    /// all; after the child has exited it does nothing.
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

/// Starts a command in its own Job Object (Windows) or process group (Unix), with stdout and stderr piped.
pub fn spawn(c: &Command) -> Result<Child> {
    let mut cmd = CommandWrap::with_new(&c.program, |cmd| {
        cmd.args(&c.args).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(if c.stdin { Stdio::piped() } else { Stdio::null() });
        if let Some(cwd) = &c.cwd {
            cmd.current_dir(cwd);
        }
        for k in &c.env_remove {
            cmd.env_remove(k);
        }
        for (k, v) in &c.env {
            cmd.env(k, v);
        }
    });
    #[cfg(windows)]
    cmd.wrap(process_wrap::tokio::JobObject);
    #[cfg(unix)]
    cmd.wrap(process_wrap::tokio::ProcessGroup::leader());
    cmd.wrap(KillOnDrop);
    cmd.spawn().map(Child).with_context(|| format!("start {}", c.program.display()))
}

/// Every line the child writes, stdout and stderr interleaved as they arrive. The channel closes when both end, which
/// is when every process holding them (grandchildren too) has exited.
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

/// Runs a command to the end and returns its stdout, or an error with its stderr. Dropped early, it kills the tree.
pub async fn output(c: &Command) -> Result<String> {
    let mut child = spawn(c)?;
    let mut out = child.stdout().take().context("no stdout")?;
    let mut err = child.stderr().take().context("no stderr")?;
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let (r1, r2) = tokio::join!(out.read_to_end(&mut o), err.read_to_end(&mut e));
    r1?;
    r2?;
    let status = child.wait().await?;
    if !status.success() {
        bail!("{} failed ({status}): {}", c.display(), String::from_utf8_lossy(&e).trim());
    }
    Ok(String::from_utf8_lossy(&o).into_owned())
}

/// Runs a command to the end, handing each line of its output to `log`. A cancel kills the tree and returns
/// [`Cancelled`]. The exit status is returned as is: the caller decides what a failure means.
pub async fn run_logged(
    c: &Command,
    cancel: &CancellationToken,
    log: &mut (dyn FnMut(&str) + Send),
) -> Result<ExitStatus> {
    let mut child = spawn(c)?;
    let mut lines = lines(&mut child);
    loop {
        tokio::select! {
            line = lines.recv() => match line {
                Some(line) => log(&line),
                None => break,
            },
            _ = cancel.cancelled() => {
                child.kill_tree().await?;
                return Err(Cancelled.into());
            }
        }
    }
    Ok(child.wait().await?)
}

/// Looks a program up on `PATH`, as the shell would, adding the platform's executable suffix.
pub fn which(program: &str) -> Option<PathBuf> {
    let name = crate::exe(program);
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(&name)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn shell(script: &str) -> Command {
        Command::new("cmd").args(["/C", script])
    }

    #[cfg(not(windows))]
    fn shell(script: &str) -> Command {
        Command::new("sh").args(["-c", script])
    }

    #[tokio::test]
    async fn returns_stdout_and_reports_failures() {
        let out = output(&shell("echo hello")).await.unwrap();
        assert_eq!(out.trim(), "hello");
        let err = output(&shell("echo oops 1>&2 && exit 3")).await.unwrap_err().to_string();
        assert!(err.contains("oops"), "{err}");
    }

    #[tokio::test]
    async fn passes_the_environment() {
        #[cfg(windows)]
        let c = shell("echo %DAMELESS_TEST_VAR%");
        #[cfg(not(windows))]
        let c = shell("echo $DAMELESS_TEST_VAR");
        let out = output(&c.env("DAMELESS_TEST_VAR", "seen")).await.unwrap();
        assert_eq!(out.trim(), "seen");
    }

    /// The shell starts a long-running grandchild that holds the output pipes. Killing the child must take the
    /// grandchild too, or the pipes would stay open and the line channel would not close.
    #[tokio::test]
    async fn killing_a_child_kills_its_tree() {
        #[cfg(windows)]
        let c = shell("ping -n 30 127.0.0.1 > nul & echo done");
        #[cfg(not(windows))]
        let c = shell("sleep 30 & wait");
        let mut child = spawn(&c).unwrap();
        let mut rx = lines(&mut child);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let started = std::time::Instant::now();
        child.kill_tree().await.unwrap();
        let closed = tokio::time::timeout(Duration::from_secs(5), async { while rx.recv().await.is_some() {} }).await;
        assert!(closed.is_ok(), "the grandchild kept the pipes open");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_cancel_stops_a_logged_run() {
        #[cfg(windows)]
        let c = shell("ping -n 30 127.0.0.1");
        #[cfg(not(windows))]
        let c = shell("sleep 30");
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel.cancel();
        });
        let started = std::time::Instant::now();
        let err = run_logged(&c, &token, &mut |_| {}).await.unwrap_err();
        assert!(crate::is_cancelled(&err));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn displays_a_command() {
        assert_eq!(Command::new("git").args(["log", "-1"]).display(), "git log -1");
    }
}
